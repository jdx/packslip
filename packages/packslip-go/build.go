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

// Build constructs a statement from explicit metadata and streams every input
// file once to calculate its SHA-256, SHA-512, and size.
func Build(metadata Predicate, inputs []ArtifactInput) (Statement, error) {
	if len(inputs) == 0 {
		return Statement{}, fmt.Errorf("packslip: no artifact inputs")
	}
	statement := Statement{Type: StatementType, PredicateType: ReleasePredicateType, Predicate: metadata}
	statement.Predicate.Artifacts = make([]Artifact, 0, len(inputs))
	statement.Subject = make([]Subject, 0, len(inputs))
	for _, input := range inputs {
		if input.Path == "" {
			return Statement{}, fmt.Errorf("packslip: %q has no local path", input.Artifact.Name)
		}
		file, err := os.Open(input.Path)
		if err != nil {
			return Statement{}, fmt.Errorf("packslip: open %q: %w", input.Path, err)
		}
		digest, size, hashErr := hash(file)
		closeErr := file.Close()
		if hashErr != nil {
			return Statement{}, fmt.Errorf("packslip: hash %q: %w", input.Path, hashErr)
		}
		if closeErr != nil {
			return Statement{}, fmt.Errorf("packslip: close %q: %w", input.Path, closeErr)
		}
		artifact := input.Artifact
		artifact.Size = size
		statement.Predicate.Artifacts = append(statement.Predicate.Artifacts, artifact)
		statement.Subject = append(statement.Subject, Subject{Name: artifact.Name, Digest: digest})
	}
	if err := statement.Validate(); err != nil {
		return Statement{}, err
	}
	return statement, nil
}

func hash(reader io.Reader) (Digest, int64, error) {
	first, second := sha256.New(), sha512.New()
	size, err := io.Copy(io.MultiWriter(first, second), reader)
	if err != nil {
		return Digest{}, 0, err
	}
	return Digest{SHA256: fmt.Sprintf("%x", first.Sum(nil)), SHA512: fmt.Sprintf("%x", second.Sum(nil))}, size, nil
}
