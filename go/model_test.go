package packslip

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestStatementValidityVectors(t *testing.T) {
	bytes, err := os.ReadFile(filepath.Join("..", "tests", "conformance", "statement-validity.json"))
	if os.IsNotExist(err) {
		t.Skip("shared repository conformance fixtures are unavailable in a standalone module checkout")
	}
	if err != nil {
		t.Fatal(err)
	}
	var vectors struct {
		Cases []struct {
			Name, Expect string
			Statement    json.RawMessage `json:"statement"`
		} `json:"cases"`
	}
	if err := json.Unmarshal(bytes, &vectors); err != nil {
		t.Fatal(err)
	}
	for _, tc := range vectors.Cases {
		t.Run(tc.Name, func(t *testing.T) {
			var statement Statement
			err := json.Unmarshal(tc.Statement, &statement)
			if err == nil {
				err = statement.Validate()
			}
			if tc.Expect == "accept" && err != nil {
				t.Fatalf("accepted vector rejected: %v", err)
			}
			if tc.Expect == "reject" && err == nil {
				t.Fatal("rejected vector accepted")
			}
		})
	}
}

func TestBuildStreamsAndBindsSubjects(t *testing.T) {
	dir := t.TempDir()
	file := filepath.Join(dir, "tool.tar.xz")
	asset := filepath.Join(dir, "tool.bash")
	if err := os.WriteFile(file, []byte("packslip"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(asset, []byte("complete -W tool tool"), 0o600); err != nil {
		t.Fatal(err)
	}
	statement, err := Build(Predicate{
		Project:     "github.com/example/tool",
		Version:     "0.1.0",
		PublishedAt: time.Date(2026, 10, 16, 0, 0, 0, 0, time.UTC),
		Identity: Identity{
			Scheme: "sigstore-oidc",
			KeyID:  "https://github.com/example/tool/.github/workflows/release.yml@refs/tags/v0.1.0",
			Issuer: "https://token.actions.githubusercontent.com",
		},
		Resources: []Resource{{Kind: "icon", Asset: "tool.bash", URL: "https://example.test/tool.bash"}},
	}, []ArtifactInput{{
		Path: file,
		Artifact: Artifact{
			Name: "tool.tar.xz", OS: "linux", Arch: "x86_64", Format: "tar.xz", Bin: []Bin{NewBin("tool")},
		},
	}}, BuildOptions{Assets: []AssetInput{{Name: "tool.bash", Path: asset}}})
	if err != nil {
		t.Fatal(err)
	}
	if got, want := statement.Subject[0].Digest.SHA256, "adfc4cdc45c6305db2693d67dfbedf20f8325a9e13ae4410b185fe58d4762671"; got != want {
		t.Fatalf("sha256 = %s, want %s", got, want)
	}
	if statement.Predicate.Artifacts[0].Size != 8 {
		t.Fatalf("size = %d", statement.Predicate.Artifacts[0].Size)
	}
	if len(statement.Subject) != 2 || statement.Subject[1].Name != "tool.bash" {
		t.Fatalf("asset subject = %#v", statement.Subject)
	}
}

func TestValidateRejectsBindingAndIdentityFailures(t *testing.T) {
	base := Statement{Type: StatementType, PredicateType: ReleasePredicateType, Subject: []Subject{{Name: "a.tar.xz", Digest: Digest{SHA256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}, Predicate: Predicate{Project: "github.com/example/tool", Version: "0.1.0", PublishedAt: time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC), Artifacts: []Artifact{{Name: "a.tar.xz", Size: 1, Format: "tar.xz"}}, Identity: Identity{Scheme: "sigstore-oidc", KeyID: "https://example.test/workflow", Issuer: "https://issuer.test"}}}
	if err := base.Validate(); err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		name   string
		mutate func(*Statement)
	}{
		{"duplicate subject", func(s *Statement) { s.Subject = append(s.Subject, s.Subject[0]) }},
		{"orphan artifact", func(s *Statement) { s.Predicate.Artifacts[0].Name = "other.tar.xz" }},
		{"bad sha256", func(s *Statement) { s.Subject[0].Digest.SHA256 = "ABC" }},
		{"bad identity", func(s *Statement) { s.Predicate.Identity.Issuer = "" }},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			copy := base
			copy.Subject = append([]Subject(nil), base.Subject...)
			copy.Predicate.Artifacts = append([]Artifact(nil), base.Predicate.Artifacts...)
			tc.mutate(&copy)
			if err := copy.Validate(); err == nil {
				t.Fatal("Validate accepted malformed statement")
			}
		})
	}
}

func TestValidateResourceAndKeyIdentityRules(t *testing.T) {
	base := Statement{Type: StatementType, PredicateType: ReleasePredicateType,
		Subject: []Subject{{Name: "tool", Digest: Digest{SHA256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}},
		Predicate: Predicate{Project: "github.com/example/tool", Version: "0.1.0", PublishedAt: time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC),
			Artifacts: []Artifact{{Name: "tool", Size: 1, Format: "raw", Bin: []Bin{NewBin("tool")}}},
			Identity:  Identity{Scheme: "sigstore-key", KeyID: "AABB"}}}
	if err := base.Validate(); err != nil {
		t.Fatalf("key identity should be valid: %v", err)
	}
	tests := []struct {
		name     string
		resource Resource
	}{
		{"unknown exec", Resource{Kind: "completion", Shells: []string{"bash"}, Exec: []string{"other", "{shell}"}}},
		{"archive in named bare", Resource{Kind: "man", Artifact: "tool", Archive: "tool.1"}},
		{"url not asset", Resource{Kind: "icon", Archive: "icon.svg", URL: "https://example.test/icon.svg"}},
		{"environment without exec", Resource{Kind: "icon", Archive: "icon.svg", Env: map[string]string{"X": "y"}}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			s := base
			s.Predicate.Resources = []Resource{tc.resource}
			if err := s.Validate(); err == nil {
				t.Fatal("accepted invalid resource")
			}
		})
	}
}

func TestValidateRejectsObjectBinWithoutName(t *testing.T) {
	statement := Statement{Type: StatementType, PredicateType: ReleasePredicateType,
		Subject: []Subject{{Name: "tool", Digest: Digest{SHA256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}},
		Predicate: Predicate{Project: "github.com/example/tool", Version: "0.1.0", PublishedAt: time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC),
			Artifacts: []Artifact{{Name: "tool", Size: 1, Format: "raw", Bin: []Bin{{Path: "tool"}}}},
			Identity:  Identity{Scheme: "sigstore-key", KeyID: "AABB"}}}
	if err := statement.Validate(); err == nil {
		t.Fatal("accepted object-form bin without a name")
	}
}
