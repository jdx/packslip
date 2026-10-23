package packslip

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"time"
)

const (
	// TokenEnv names the environment variable that holds an OIDC token for
	// Sigstore, taking precedence over an ambient CI credential.
	TokenEnv = "SIGSTORE_ID_TOKEN"

	githubIssuer     = "https://token.actions.githubusercontent.com"
	gitlabIssuer     = "https://gitlab.com"
	sigstoreAudience = "sigstore"
)

// ErrNoIDToken means neither SIGSTORE_ID_TOKEN nor a GitHub Actions
// id-token credential is available.
var ErrNoIDToken = errors.New("packslip: no OIDC token found: set " + TokenEnv + ", or grant the GitHub Actions job `id-token: write` permission")

// AmbientIDToken returns the OIDC token to sign with: SIGSTORE_ID_TOKEN when
// it is set, otherwise a token requested from GitHub Actions for the
// `sigstore` audience. Other CI systems should export their token as
// SIGSTORE_ID_TOKEN. It returns ErrNoIDToken when there is none.
//
// This is the only function in the module that reads the environment or makes
// an OIDC request, and nothing else calls it: pass its result to
// ProductionKeylessOptions explicitly.
func AmbientIDToken(ctx context.Context) (string, error) {
	if token := strings.TrimSpace(os.Getenv(TokenEnv)); token != "" {
		return token, nil
	}
	requestURL := os.Getenv("ACTIONS_ID_TOKEN_REQUEST_URL")
	requestToken := os.Getenv("ACTIONS_ID_TOKEN_REQUEST_TOKEN")
	if requestURL == "" || requestToken == "" {
		return "", ErrNoIDToken
	}
	u, err := url.Parse(requestURL)
	if err != nil {
		return "", fmt.Errorf("packslip: invalid ACTIONS_ID_TOKEN_REQUEST_URL: %w", err)
	}
	q := u.Query()
	q.Set("audience", sigstoreAudience)
	u.RawQuery = q.Encode()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u.String(), nil)
	if err != nil {
		return "", err
	}
	req.Header.Set("Authorization", "Bearer "+requestToken)
	resp, err := tokenClient().Do(req)
	if err != nil {
		return "", fmt.Errorf("packslip: request GitHub Actions OIDC token: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("packslip: request GitHub Actions OIDC token: %s", resp.Status)
	}
	var body struct {
		Value string `json:"value"`
	}
	if err := json.NewDecoder(io.LimitReader(resp.Body, 1<<20)).Decode(&body); err != nil {
		return "", fmt.Errorf("packslip: read GitHub Actions OIDC token: %w", err)
	}
	if body.Value == "" {
		return "", errors.New("packslip: GitHub Actions returned an empty OIDC token")
	}
	return body.Value, nil
}

// IdentityFromToken returns the statement identity that Fulcio will write in
// the certificate it issues for an OIDC token: the workflow URI for GitHub
// Actions and GitLab CI tokens, otherwise the verified email or the subject.
//
// The token is not verified. Fulcio does that when issuing the certificate,
// and SignKeyless refuses a certificate whose identity differs from the
// statement's, so a wrong guess fails before anything is logged.
func IdentityFromToken(token string) (Identity, error) {
	claims, err := decodeClaims(token)
	if err != nil {
		return Identity{}, err
	}
	if claims.Issuer == "" {
		return Identity{}, errors.New("packslip: the OIDC token has no issuer")
	}
	var keyID string
	switch {
	case claims.Issuer == githubIssuer && claims.JobWorkflowRef != "":
		keyID = "https://github.com/" + claims.JobWorkflowRef
	case claims.CIConfigRefURI != "":
		keyID = "https://" + claims.CIConfigRefURI
	case claims.Email != "" && claims.EmailVerified:
		keyID = claims.Email
	case claims.Subject != "":
		keyID = claims.Subject
	default:
		return Identity{}, errors.New("packslip: the OIDC token has no usable identity claim")
	}
	return Identity{Scheme: "sigstore-oidc", KeyID: keyID, Issuer: claims.Issuer}, nil
}

// tokenClaims holds the OIDC token claims this module reads.
type tokenClaims struct {
	Issuer         string `json:"iss"`
	Subject        string `json:"sub"`
	Email          string `json:"email"`
	EmailVerified  bool   `json:"email_verified"`
	JobWorkflowRef string `json:"job_workflow_ref"`
	CIConfigRefURI string `json:"ci_config_ref_uri"`
	Repository     string `json:"repository"`
	ProjectPath    string `json:"project_path"`
}

// decodeClaims reads a token's claims without verifying it.
func decodeClaims(token string) (tokenClaims, error) {
	parts := strings.Split(token, ".")
	if len(parts) != 3 {
		return tokenClaims{}, errors.New("packslip: the OIDC token is not a JWT")
	}
	payload, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		return tokenClaims{}, fmt.Errorf("packslip: decode the OIDC token: %w", err)
	}
	var claims tokenClaims
	if err := json.Unmarshal(payload, &claims); err != nil {
		return tokenClaims{}, fmt.Errorf("packslip: decode the OIDC token: %w", err)
	}
	return claims, nil
}

// CheckProjectSigner reports whether an OIDC token can sign a statement for
// project, before anything is signed or logged. A github.com project needs a
// GitHub Actions token whose repository is the project's repository, and a
// gitlab.com project needs a GitLab CI token whose project path is the
// project. That is the certificate's Source Repository URI, so a reusable
// workflow of another repository is fine, as the specification allows
// ([Repository identity and renames]). The repository is compared without
// regard to case, and a monorepo subpath is not checked. Any other project is
// a domain project, whose signer its vendor pins, so it always passes.
//
// The token is not verified, and the check is not verification: Fulcio
// certifies the token, and consumers verify the bundle. A project that
// follows a repository rename passes verification but fails this check.
//
// [Repository identity and renames]: https://packslip.dev/release/v1/#repository-identity-and-renames
func CheckProjectSigner(project, token string) error {
	claims, err := decodeClaims(token)
	if err != nil {
		return err
	}
	switch {
	case strings.HasPrefix(project, "github.com/"):
		if claims.Issuer != githubIssuer {
			return fmt.Errorf("packslip: project %q is signed from GitHub Actions, but the OIDC token is from %q", project, claims.Issuer)
		}
		if strings.Count(claims.Repository, "/") != 1 {
			return errors.New("packslip: the GitHub Actions OIDC token has no repository claim")
		}
		repo := "github.com/" + claims.Repository
		if len(project) < len(repo) || !strings.EqualFold(project[:len(repo)], repo) || (len(project) > len(repo) && project[len(repo)] != '/') {
			return fmt.Errorf("packslip: project %q does not belong to the GitHub Actions repository %q that signs", project, repo)
		}
	case strings.HasPrefix(project, "gitlab.com/"):
		if claims.Issuer != gitlabIssuer {
			return fmt.Errorf("packslip: project %q is signed from GitLab CI, but the OIDC token is from %q", project, claims.Issuer)
		}
		if claims.ProjectPath == "" {
			return errors.New("packslip: the GitLab CI OIDC token has no project_path claim")
		}
		if !strings.EqualFold(project, "gitlab.com/"+claims.ProjectPath) {
			return fmt.Errorf("packslip: project %q is not the GitLab CI project %q that signs", project, "gitlab.com/"+claims.ProjectPath)
		}
	}
	return nil
}

// tokenClient bounds the whole token request, including reading the body.
// It keeps Go's default HTTP/2 negotiation: forcing HTTP/1.1 makes GitHub's
// token endpoint close the connection (EOF) on hosted runners.
func tokenClient() *http.Client {
	return &http.Client{Timeout: 30 * time.Second}
}
