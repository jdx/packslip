package packslip

import "strings"

// ArchFromGo returns the packslip architecture for a GOARCH. goarm is
// GOARM and only matters for "arm"; empty means 6, the Go default, and a
// floating-point option such as ",softfloat" is ignored. A
// universal macOS binary (GOARCH "all" in GoReleaser) returns "", because an
// absent arch means the build runs on every architecture.
//
// Architectures outside the documented vocabulary use Rust's spelling
// (`mipsel`, `mips64el`, `powerpc64`), so no consumer takes them for another
// one. An unknown GOARCH is returned unchanged.
func ArchFromGo(goarch, goarm string) string {
	switch goarch {
	case "amd64":
		return "x86_64"
	case "arm64":
		return "aarch64"
	case "386":
		return "i686"
	case "arm":
		// GOARM may carry a float option, as in "7,softfloat".
		goarm, _, _ = strings.Cut(goarm, ",")
		if goarm == "" {
			goarm = "6"
		}
		return "armv" + goarm
	case "ppc64le":
		return "powerpc64le"
	case "ppc64":
		return "powerpc64"
	case "loong64":
		return "loongarch64"
	case "mipsle":
		return "mipsel"
	case "mips64le":
		return "mips64el"
	case "all":
		return ""
	}
	return goarch
}

// NewBareBin names the executable inside a bare artifact, one whose Format is
// raw, gz, xz, zst or bz2. Its path is the artifact's own name without the
// compression suffix, and command is the name it gets on PATH (a trailing
// .exe is dropped).
func NewBareBin(artifactName, format, command string) Bin {
	return NamedBin(bareFile(artifactName, format), command)
}
