// conformance emits a deterministic release statement for the Rust/Go wire
// compatibility test. It is a test helper, not a publisher CLI.
package main

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"flag"
	"fmt"
	"time"

	packslip "github.com/jdx/packslip/go"
	protocommon "github.com/sigstore/protobuf-specs/gen/pb-go/common/v1"
	"github.com/sigstore/sigstore-go/pkg/sign"
	"google.golang.org/protobuf/encoding/protojson"
)

func main() {
	signed := flag.Bool("signed", false, "emit an offline key-signed test bundle")
	flag.Parse()
	if *signed {
		emitSigned()
		return
	}
	statement := packslip.Statement{
		Type:          packslip.StatementType,
		PredicateType: packslip.ReleasePredicateType,
		Subject: []packslip.Subject{{Name: "tool-linux-x64.tar.xz", Digest: packslip.Digest{
			SHA256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
		}}},
		Predicate: packslip.Predicate{
			Project:     "github.com/example/tool",
			Version:     "0.1.0",
			PublishedAt: time.Date(2026, 10, 16, 0, 0, 0, 0, time.UTC),
			Artifacts: []packslip.Artifact{{
				Name: "tool-linux-x64.tar.xz", OS: "linux", Arch: "x86_64", Format: "tar.xz", Size: 1, Bin: []packslip.Bin{packslip.NewBin("tool")},
			}},
			Identity: packslip.Identity{Scheme: "sigstore-oidc", KeyID: "https://github.com/example/tool/.github/workflows/release.yml@refs/tags/v0.1.0", Issuer: "https://token.actions.githubusercontent.com"},
		},
	}
	output, err := statement.CanonicalJSON()
	if err != nil {
		panic(err)
	}
	fmt.Println(string(output))
}

// emitSigned is deliberately a conformance helper, not a public signing API.
// It lets the Rust verifier check a sigstore-go DSSE bundle offline with a
// throw-away Ed25519 key; production publishers must use SignKeyless.
func emitSigned() {
	key, err := sign.NewEphemeralKeypair(&sign.EphemeralKeypairOptions{Algorithm: protocommon.PublicKeyDetails_PKIX_ED25519})
	if err != nil {
		panic(err)
	}
	pub := key.GetPublicKey().(ed25519.PublicKey)
	idBytes := sha256.Sum256(pub)
	keyID := fmt.Sprintf("%016X", binary.LittleEndian.Uint64(idBytes[:8]))
	statement := packslip.Statement{Type: packslip.StatementType, PredicateType: packslip.ReleasePredicateType,
		Subject:   []packslip.Subject{{Name: "tool", Digest: packslip.Digest{SHA256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}},
		Predicate: packslip.Predicate{Project: "github.com/example/tool", Version: "0.1.0", PublishedAt: time.Date(2026, 10, 16, 0, 0, 0, 0, time.UTC), Identity: packslip.Identity{Scheme: "sigstore-key", KeyID: keyID}, Artifacts: []packslip.Artifact{{Name: "tool", Size: 1, Format: "raw", Bin: []packslip.Bin{packslip.NewBin("tool")}}}}}
	payload, err := statement.CanonicalJSON()
	if err != nil {
		panic(err)
	}
	bundle, err := sign.Bundle(&sign.DSSEData{Data: payload, PayloadType: packslip.DSSEPayloadType}, key, sign.BundleOptions{})
	if err != nil {
		panic(err)
	}
	encoded, err := protojson.Marshal(bundle)
	if err != nil {
		panic(err)
	}
	mini := append([]byte("Ed"), idBytes[:8]...)
	mini = append(mini, pub...)
	result, err := json.Marshal(map[string]string{"bundle": string(encoded), "public_key": "untrusted comment: minisign public key " + keyID + "\n" + base64.StdEncoding.EncodeToString(mini) + "\n"})
	if err != nil {
		panic(err)
	}
	fmt.Println(string(result))
}
