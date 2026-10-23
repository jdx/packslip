package packslip

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
)

func fakeJWT(t *testing.T, claims map[string]any) string {
	t.Helper()
	payload, err := json.Marshal(claims)
	if err != nil {
		t.Fatal(err)
	}
	return "e30." + base64.RawURLEncoding.EncodeToString(payload) + ".sig"
}

func TestIdentityFromToken(t *testing.T) {
	for name, tc := range map[string]struct {
		claims map[string]any
		want   Identity
	}{
		"github workflow": {
			map[string]any{"iss": githubIssuer, "job_workflow_ref": "acme/tool/.github/workflows/release.yml@refs/tags/v1.0.0", "sub": "repo:acme/tool"},
			Identity{Scheme: "sigstore-oidc", Issuer: githubIssuer, KeyID: "https://github.com/acme/tool/.github/workflows/release.yml@refs/tags/v1.0.0"},
		},
		"gitlab pipeline": {
			map[string]any{"iss": "https://gitlab.com", "ci_config_ref_uri": "gitlab.com/acme/tool//.gitlab-ci.yml@refs/tags/v1.0.0"},
			Identity{Scheme: "sigstore-oidc", Issuer: "https://gitlab.com", KeyID: "https://gitlab.com/acme/tool//.gitlab-ci.yml@refs/tags/v1.0.0"},
		},
		"verified email": {
			map[string]any{"iss": "https://accounts.example.com", "email": "me@example.com", "email_verified": true, "sub": "123"},
			Identity{Scheme: "sigstore-oidc", Issuer: "https://accounts.example.com", KeyID: "me@example.com"},
		},
		"unverified email falls back to subject": {
			map[string]any{"iss": "https://accounts.example.com", "email": "me@example.com", "sub": "123"},
			Identity{Scheme: "sigstore-oidc", Issuer: "https://accounts.example.com", KeyID: "123"},
		},
	} {
		t.Run(name, func(t *testing.T) {
			got, err := IdentityFromToken(fakeJWT(t, tc.claims))
			if err != nil || got != tc.want {
				t.Fatalf("got %+v, %v; want %+v", got, err, tc.want)
			}
		})
	}
	for name, bad := range map[string]string{
		"not a jwt":   "nope",
		"bad payload": "a.!!!.c",
		"no issuer":   fakeJWT(t, map[string]any{"sub": "1"}),
		"no identity": fakeJWT(t, map[string]any{"iss": "https://x.example.com"}),
	} {
		t.Run(name, func(t *testing.T) {
			if _, err := IdentityFromToken(bad); err == nil {
				t.Fatal("expected an error")
			}
		})
	}
}

func TestAmbientIDTokenFromEnv(t *testing.T) {
	t.Setenv(TokenEnv, " abc \n")
	got, err := AmbientIDToken(context.Background())
	if err != nil || got != "abc" {
		t.Fatalf("got %q, %v", got, err)
	}
}

func TestAmbientIDTokenMissing(t *testing.T) {
	t.Setenv(TokenEnv, "")
	t.Setenv("ACTIONS_ID_TOKEN_REQUEST_URL", "")
	t.Setenv("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "")
	if _, err := AmbientIDToken(context.Background()); !errors.Is(err, ErrNoIDToken) {
		t.Fatalf("got %v, want ErrNoIDToken", err)
	}
}

func TestAmbientIDTokenFromGitHubActions(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer request-token" || r.URL.Query().Get("audience") != "sigstore" || r.URL.Query().Get("existing") != "1" {
			http.Error(w, "bad request", http.StatusBadRequest)
			return
		}
		_, _ = w.Write([]byte(`{"value":"from-github"}`))
	}))
	defer server.Close()
	t.Setenv(TokenEnv, "")
	t.Setenv("ACTIONS_ID_TOKEN_REQUEST_URL", server.URL+"/token?existing=1")
	t.Setenv("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "request-token")
	got, err := AmbientIDToken(context.Background())
	if err != nil || got != "from-github" {
		t.Fatalf("got %q, %v", got, err)
	}
}

func TestAmbientIDTokenGitHubFailure(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "no", http.StatusForbidden)
	}))
	defer server.Close()
	t.Setenv(TokenEnv, "")
	t.Setenv("ACTIONS_ID_TOKEN_REQUEST_URL", server.URL)
	t.Setenv("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "request-token")
	if _, err := AmbientIDToken(context.Background()); err == nil {
		t.Fatal("expected an error")
	}
}

func TestCheckProjectSigner(t *testing.T) {
	github := func(repo string) string {
		return fakeJWT(t, map[string]any{"iss": githubIssuer, "repository": repo, "job_workflow_ref": "shared/ci/.github/workflows/release.yml@refs/heads/main"})
	}
	gitlab := fakeJWT(t, map[string]any{"iss": gitlabIssuer, "project_path": "acme/group/tool"})
	other := fakeJWT(t, map[string]any{"iss": "https://accounts.example.com", "email": "me@example.com", "email_verified": true})
	for name, tc := range map[string]struct {
		project, token string
		ok             bool
	}{
		"github repository":            {"github.com/acme/tool", github("acme/tool"), true},
		"github reusable workflow":     {"github.com/acme/tool", github("acme/tool"), true},
		"github monorepo subpath":      {"github.com/acme/tool/crates/cli", github("acme/tool"), true},
		"github letter case":           {"github.com/ACME/Tool/cli", github("acme/tool"), true},
		"github other repository":      {"github.com/acme/other", github("acme/tool"), false},
		"github repository prefix":     {"github.com/acme/toolbox", github("acme/tool"), false},
		"github no repository claim":   {"github.com/acme/tool", fakeJWT(t, map[string]any{"iss": githubIssuer}), false},
		"github project, gitlab token": {"github.com/acme/tool", gitlab, false},
		"github project, other token":  {"github.com/acme/tool", other, false},
		"gitlab project":               {"gitlab.com/acme/group/tool", gitlab, true},
		"gitlab letter case":           {"gitlab.com/Acme/Group/Tool", gitlab, true},
		"gitlab other project":         {"gitlab.com/acme/group/other", gitlab, false},
		"gitlab project, github token": {"gitlab.com/acme/group/tool", github("acme/tool"), false},
		"domain project":               {"mytool.example.com", other, true},
		"domain project, ci token":     {"mytool.example.com", github("acme/tool"), true},
	} {
		t.Run(name, func(t *testing.T) {
			if err := CheckProjectSigner(tc.project, tc.token); (err == nil) != tc.ok {
				t.Fatalf("CheckProjectSigner(%q) = %v, want ok=%v", tc.project, err, tc.ok)
			}
		})
	}
	if err := CheckProjectSigner("github.com/acme/tool", "not-a-jwt"); err == nil {
		t.Fatal("a malformed token passed")
	}
}
