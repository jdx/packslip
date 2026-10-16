package packslip

import (
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/x509"
	"crypto/x509/pkix"
	"math/big"
	"net/url"
	"testing"
	"time"

	"github.com/sigstore/sigstore-go/pkg/fulcio/certificate"
)

func TestSignKeylessRejectsKeyIdentityBeforeNetworkConfiguration(t *testing.T) {
	statement := Statement{Type: StatementType, PredicateType: ReleasePredicateType,
		Subject: []Subject{{Name: "tool", Digest: Digest{SHA256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}},
		Predicate: Predicate{Project: "github.com/acme/tool", Version: "0.1.0", PublishedAt: time.Now().UTC(),
			Identity: Identity{Scheme: "sigstore-key", KeyID: "AABB"}, Artifacts: []Artifact{{Name: "tool", Size: 1, Format: "raw"}}}}
	if _, err := SignKeyless(statement, KeylessOptions{}); err == nil {
		t.Fatal("keyless signing accepted a sigstore-key statement")
	}
}

func TestCertificateIdentityBindingRejectsSANAndIssuerMismatch(t *testing.T) {
	cert := testFulcioCertificate(t, "https://github.com/acme/tool/.github/workflows/release.yml@refs/tags/v0.1.0", "https://token.actions.githubusercontent.com")
	identity := Identity{Scheme: "sigstore-oidc", KeyID: "https://github.com/acme/tool/.github/workflows/release.yml@refs/tags/v0.1.0", Issuer: "https://token.actions.githubusercontent.com"}
	if err := validateCertificateIdentity(cert, identity); err != nil {
		t.Fatalf("matching Fulcio identity rejected: %v", err)
	}
	identity.KeyID = "https://github.com/acme/other/.github/workflows/release.yml@refs/tags/v0.1.0"
	if err := validateCertificateIdentity(cert, identity); err == nil {
		t.Fatal("SAN mismatch accepted")
	}
	identity.KeyID = "https://github.com/acme/tool/.github/workflows/release.yml@refs/tags/v0.1.0"
	identity.Issuer = "https://issuer.example.test"
	if err := validateCertificateIdentity(cert, identity); err == nil {
		t.Fatal("issuer mismatch accepted")
	}
}

func testFulcioCertificate(t *testing.T, identity, issuer string) *x509.Certificate {
	t.Helper()
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	u, err := url.Parse(identity)
	if err != nil {
		t.Fatal(err)
	}
	template := &x509.Certificate{
		SerialNumber: big.NewInt(1),
		Subject:      pkix.Name{CommonName: "offline-test"},
		NotBefore:    time.Now().Add(-time.Minute), NotAfter: time.Now().Add(time.Minute),
		URIs:            []*url.URL{u},
		ExtraExtensions: []pkix.Extension{{Id: certificate.OIDIssuer, Value: []byte(issuer)}},
	}
	der, err := x509.CreateCertificate(rand.Reader, template, template, &key.PublicKey, key)
	if err != nil {
		t.Fatal(err)
	}
	cert, err := x509.ParseCertificate(der)
	if err != nil {
		t.Fatal(err)
	}
	return cert
}
