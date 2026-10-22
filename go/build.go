package packslip

import (
	"crypto/sha256"
	"crypto/sha512"
	"fmt"
	"io"
	"os"
)

// ArtifactInput binds explicit release metadata to a file that will be hashed.
// Name remains metadata, so a caller can use an upload name unrelated to the
// local path.
type ArtifactInput struct {
	Artifact Artifact
	Path     string
}

// AssetInput binds the uploaded asset name used by a resource to a local file.
// Assets become statement subjects but are not release artifacts.
type AssetInput struct {
	Name string
	Path string
}

// BuildOptions supplies optional inputs that do not change the artifact list.
type BuildOptions struct {
	Assets []AssetInput
}

// Build constructs a statement from explicit metadata and streams every input
// file once to calculate its SHA-256, SHA-512, and size.
func Build(metadata Predicate, inputs []ArtifactInput, buildOptions ...BuildOptions) (Statement, error) {
	if len(inputs) == 0 {
		return Statement{}, fmt.Errorf("packslip: no artifact inputs")
	}
	if len(buildOptions) > 1 {
		return Statement{}, fmt.Errorf("packslip: at most one build options value is allowed")
	}
	var options BuildOptions
	if len(buildOptions) == 1 {
		options = buildOptions[0]
	}
	statement := Statement{Type: StatementType, PredicateType: ReleasePredicateType, Predicate: metadata}
	statement.Predicate.Artifacts = make([]Artifact, 0, len(inputs))
	statement.Subject = make([]Subject, 0, len(inputs)+len(options.Assets))
	for _, input := range inputs {
		if input.Path == "" {
			return Statement{}, fmt.Errorf("packslip: %q has no local path", input.Artifact.Name)
		}
		if err := checkRequirementMinimums(input.Artifact); err != nil {
			return Statement{}, err
		}
		digest, size, err := hashPath(input.Path)
		if err != nil {
			return Statement{}, err
		}
		artifact := input.Artifact
		artifact.Size = size
		statement.Predicate.Artifacts = append(statement.Predicate.Artifacts, artifact)
		statement.Subject = append(statement.Subject, Subject{Name: artifact.Name, Digest: digest})
	}
	for _, input := range options.Assets {
		if input.Name == "" || input.Path == "" {
			return Statement{}, fmt.Errorf("packslip: asset requires both name and local path")
		}
		digest, _, err := hashPath(input.Path)
		if err != nil {
			return Statement{}, err
		}
		statement.Subject = append(statement.Subject, Subject{Name: input.Name, Digest: digest})
	}
	if err := statement.Validate(); err != nil {
		return Statement{}, err
	}
	return statement, nil
}

// checkRequirementMinimums holds a new artifact's os_min and glibc_min to
// the grammar of a required bin's min. Validate accepts other spellings
// there, which a consumer treats as an unknown check, because packslips
// published before producers checked them may carry them.
func checkRequirementMinimums(artifact Artifact) error {
	if artifact.Requires == nil {
		return nil
	}
	for _, m := range []struct{ field, value string }{
		{"os_min", artifact.Requires.OSMin},
		{"glibc_min", artifact.Requires.GlibcMin},
	} {
		if m.value != "" && !validMin(m.value) {
			return fmt.Errorf("packslip: artifact %q requires %s %q; a minimum starts with a digit and holds only letters, digits, ., -, and +, as in 2.31 or 10.0.17763", artifact.Name, m.field, m.value)
		}
	}
	return nil
}

func hashPath(path string) (Digest, int64, error) {
	file, err := os.Open(path)
	if err != nil {
		return Digest{}, 0, fmt.Errorf("packslip: open %q: %w", path, err)
	}
	digest, size, hashErr := hash(file)
	closeErr := file.Close()
	if hashErr != nil {
		return Digest{}, 0, fmt.Errorf("packslip: hash %q: %w", path, hashErr)
	}
	if closeErr != nil {
		return Digest{}, 0, fmt.Errorf("packslip: close %q: %w", path, closeErr)
	}
	return digest, size, nil
}

func hash(reader io.Reader) (Digest, int64, error) {
	first, second := sha256.New(), sha512.New()
	size, err := io.Copy(io.MultiWriter(first, second), reader)
	if err != nil {
		return Digest{}, 0, err
	}
	return Digest{SHA256: fmt.Sprintf("%x", first.Sum(nil)), SHA512: fmt.Sprintf("%x", second.Sum(nil))}, size, nil
}
