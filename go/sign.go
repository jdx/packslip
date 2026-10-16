package packslip

import (
	"context"
	"crypto/x509"
	"encoding/pem"
	"fmt"
	"time"

	"github.com/sigstore/sigstore-go/pkg/fulcio/certificate"
	"github.com/sigstore/sigstore-go/pkg/root"
	"github.com/sigstore/sigstore-go/pkg/sign"
	"github.com/sigstore/sigstore-go/pkg/tuf"
	"google.golang.org/protobuf/encoding/protojson"
)

// KeylessOptions names every authority used for signing. Supplying all fields
// is the most explicit option for private Sigstore deployments. Production
// Options obtains these values from the public-good instance's signed TUF
// metadata instead of embedding staging endpoints.
type KeylessOptions struct {
	IDToken             string
	Keypair             sign.Keypair
	CertificateProvider sign.CertificateProvider
	TransparencyLogs    []sign.Transparency
	TrustedRoot         root.TrustedMaterial
	Context             context.Context
}

// identityBoundProvider rejects a certificate whose OIDC identity differs from
// the statement before sigstore-go can upload an entry to a transparency log.
type identityBoundProvider struct {
	inner    sign.CertificateProvider
	identity Identity
}

func (p identityBoundProvider) GetCertificate(ctx context.Context, keypair sign.Keypair, options *sign.CertificateProviderOptions) ([]byte, error) {
	der, err := p.inner.GetCertificate(ctx, keypair, options)
	if err != nil {
		return nil, err
	}
	cert, err := parseIssuedCertificate(der)
	if err != nil {
		return nil, fmt.Errorf("packslip: parse issued certificate: %w", err)
	}
	if err := validateCertificateIdentity(cert, p.identity); err != nil {
		return nil, err
	}
	return der, nil
}

func parseIssuedCertificate(data []byte) (*x509.Certificate, error) {
	if block, _ := pem.Decode(data); block != nil {
		data = block.Bytes
	}
	return x509.ParseCertificate(data)
}

// ProductionKeylessOptions resolves the current Sigstore public-good Fulcio,
// Rekor, and trust root from production TUF metadata. idToken is deliberately
// passed by the caller: this package never reads an environment variable or
// performs an OIDC exchange on its own.
func ProductionKeylessOptions(ctx context.Context, idToken string) (KeylessOptions, error) {
	if idToken == "" {
		return KeylessOptions{}, fmt.Errorf("packslip: an OIDC ID token is required")
	}
	client, err := tuf.DefaultClient()
	if err != nil {
		return KeylessOptions{}, fmt.Errorf("packslip: initialize production Sigstore TUF: %w", err)
	}
	trusted, err := root.GetTrustedRoot(client)
	if err != nil {
		return KeylessOptions{}, fmt.Errorf("packslip: load production Sigstore trusted root: %w", err)
	}
	config, err := root.GetSigningConfig(client)
	if err != nil {
		return KeylessOptions{}, fmt.Errorf("packslip: load production Sigstore signing configuration: %w", err)
	}
	fulcio, err := root.SelectService(config.FulcioCertificateAuthorityURLs(), sign.FulcioAPIVersions, time.Now())
	if err != nil {
		return KeylessOptions{}, fmt.Errorf("packslip: select Fulcio service: %w", err)
	}
	rekors, err := root.SelectServices(config.RekorLogURLs(), config.RekorLogURLsConfig(), sign.RekorAPIVersions, time.Now())
	if err != nil {
		return KeylessOptions{}, fmt.Errorf("packslip: select Rekor service: %w", err)
	}
	logs := make([]sign.Transparency, 0, len(rekors))
	for _, service := range rekors {
		logs = append(logs, sign.NewRekor(&sign.RekorOptions{BaseURL: service.URL, Version: service.MajorAPIVersion, Timeout: 90 * time.Second, Retries: 1}))
	}
	keypair, err := sign.NewEphemeralKeypair(nil)
	if err != nil {
		return KeylessOptions{}, fmt.Errorf("packslip: create ephemeral keypair: %w", err)
	}
	return KeylessOptions{IDToken: idToken, Keypair: keypair, CertificateProvider: sign.NewFulcio(&sign.FulcioOptions{BaseURL: fulcio.URL, Timeout: 30 * time.Second, Retries: 1}), TransparencyLogs: logs, TrustedRoot: trusted, Context: ctx}, nil
}

// SignKeyless produces a DSSE Sigstore bundle for a valid release statement.
// The returned JSON is a bundle, not merely a parsed statement: sigstore-go
// verifies it against TrustedRoot before returning when one is supplied.
func SignKeyless(statement Statement, options KeylessOptions) ([]byte, error) {
	payload, err := statement.CanonicalJSON()
	if err != nil {
		return nil, err
	}
	if statement.Predicate.Identity.Scheme != "sigstore-oidc" || statement.Predicate.Identity.KeyID == "" || statement.Predicate.Identity.Issuer == "" {
		return nil, fmt.Errorf("packslip: keyless signing requires a sigstore-oidc statement identity with key_id and issuer")
	}
	if options.IDToken == "" || options.Keypair == nil || options.CertificateProvider == nil || len(options.TransparencyLogs) == 0 || options.TrustedRoot == nil {
		return nil, fmt.Errorf("packslip: keyless signing requires ID token, keypair, certificate provider, transparency log, and trusted root")
	}
	bundle, err := sign.Bundle(&sign.DSSEData{Data: payload, PayloadType: DSSEPayloadType}, options.Keypair, sign.BundleOptions{
		CertificateProvider:        identityBoundProvider{inner: options.CertificateProvider, identity: statement.Predicate.Identity},
		CertificateProviderOptions: &sign.CertificateProviderOptions{IDToken: options.IDToken},
		TransparencyLogs:           options.TransparencyLogs,
		TrustedRoot:                options.TrustedRoot,
		Context:                    options.Context,
	})
	if err != nil {
		return nil, fmt.Errorf("packslip: sign statement: %w", err)
	}
	// A verified bundle alone is insufficient: its certificate must be the
	// identity the signed predicate claims. Otherwise a different workflow's
	// valid OIDC token could make a misleading release statement.
	leaf := bundle.GetVerificationMaterial().GetCertificate()
	if leaf == nil {
		return nil, fmt.Errorf("packslip: keyless signing returned no certificate")
	}
	cert, err := x509.ParseCertificate(leaf.GetRawBytes())
	if err != nil {
		return nil, fmt.Errorf("packslip: parse issued certificate: %w", err)
	}
	if err := validateCertificateIdentity(cert, statement.Predicate.Identity); err != nil {
		return nil, err
	}
	encoded, err := protojson.Marshal(bundle)
	if err != nil {
		return nil, fmt.Errorf("packslip: encode bundle: %w", err)
	}
	return encoded, nil
}

// validateCertificateIdentity binds an already Sigstore-verified Fulcio leaf
// certificate to the identity that this statement declares. It is intentionally
// separate from verification: callers must never use it to treat an arbitrary
// certificate as trusted.
func validateCertificateIdentity(cert *x509.Certificate, identity Identity) error {
	summary, err := certificate.SummarizeCertificate(cert)
	if err != nil {
		return fmt.Errorf("packslip: summarize issued certificate: %w", err)
	}
	if summary.SubjectAlternativeName != identity.KeyID || summary.Extensions.Issuer != identity.Issuer {
		return fmt.Errorf("packslip: issued certificate identity %q from %q does not match statement identity %q from %q", summary.SubjectAlternativeName, summary.Extensions.Issuer, identity.KeyID, identity.Issuer)
	}
	return nil
}
