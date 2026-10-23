package packslip

import "strings"

// ArchFromGo returns the packslip architecture for a GOARCH. goarm is
// GOARM and only matters for "arm"; empty means 6, GoReleaser's default, and a
// floating-point option such as ",softfloat" is ignored. Callers using raw Go
// cross-build settings must pass the effective GOARM, since an unset
// cross-build defaults to 7. A universal macOS binary (GOARCH "all" in GoReleaser) returns "", because an
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

// VariantFromGo returns the packslip variant for a Go build, so that builds
// of one platform at different CPU levels stay distinct. level is the Go
// setting for goarch: GOAMD64, GO386, GOARM64, GOMIPS, GOPPC64, GORISCV64, or
// for "arm" the GOARM value with its float option, as in "7,softfloat".
//
// A build at Go's default level has no variant, so a consumer that does not
// ask for one gets the build that runs everywhere; so does an unset level.
// A GOARM64 option list such as "v8.2,lse" becomes "v8.2-lse", because a
// variant cannot contain a comma. A soft-float ARM build, or a MIPS build with
// GOMIPS=softfloat or GO386=softfloat, is "softfloat". GOARM=5 is soft-float
// already, so it has no variant, and GOARM=5,hardfloat is "5-hardfloat".
func VariantFromGo(goarch, level string) string {
	var def string
	switch goarch {
	case "amd64":
		def = "v1"
	case "386":
		def = "sse2"
	case "arm64":
		def = "v8.0"
	case "mips", "mipsle", "mips64", "mips64le":
		def = "hardfloat"
	case "ppc64", "ppc64le":
		def = "power8"
	case "riscv64":
		def = "rva20u64"
	case "arm":
		goarm, float, _ := strings.Cut(level, ",")
		if goarm == "5" && float == "hardfloat" {
			return "5-hardfloat"
		}
		if float == "softfloat" && goarm != "5" {
			return "softfloat"
		}
		return ""
	default:
		return ""
	}
	if level == "" || level == def {
		return ""
	}
	return strings.ReplaceAll(level, ",", "-")
}

// VariantFromRustABI returns the packslip variant for the ABI of a Rust
// target, such as "gnueabihf" in arm-unknown-linux-gnueabihf. Rust spells the
// soft-float ARM ABIs "gnueabi" and "musleabi", where the hard-float ones end
// in "hf"; those are "softfloat", and every other ABI has no variant. A glibc
// version suffix, as in "gnueabi.2.17", is ignored.
func VariantFromRustABI(abi string) string {
	abi, _, _ = strings.Cut(abi, ".")
	switch abi {
	case "gnueabi", "musleabi":
		return "softfloat"
	}
	return ""
}

// GoTarget is the Go build settings of one artifact. Fill in the settings the
// build used and leave the rest empty.
type GoTarget struct {
	GOARCH string
	// GOARM is the GOARM value, which may carry a float option, as in
	// "7,softfloat". It only matters for "arm".
	GOARM     string
	GOAMD64   string
	GO386     string
	GOARM64   string
	GOMIPS    string
	GOPPC64   string
	GORISCV64 string
	// FloatABI is the floating-point ABI of an "arm" build when the builder
	// records it apart from GOARM: "softfloat" or "hardfloat" from Go, or a
	// Rust or Zig target ABI such as "gnueabi" or "gnueabihf". It overrides a
	// float option in GOARM.
	FloatABI string
}

// Arch returns the packslip architecture, as ArchFromGo does.
func (t GoTarget) Arch() string {
	return ArchFromGo(t.GOARCH, t.GOARM)
}

// Variant returns the packslip variant for the setting that applies to
// GOARCH, as VariantFromGo does. For "arm" it also reads FloatABI, with
// VariantFromRustABI for a Rust or Zig ABI. A GOARM=5 build is soft-float
// already, so it has no variant unless its float option says hardfloat.
func (t GoTarget) Variant() string {
	var level string
	switch t.GOARCH {
	case "amd64":
		level = t.GOAMD64
	case "386":
		level = t.GO386
	case "arm64":
		level = t.GOARM64
	case "mips", "mipsle", "mips64", "mips64le":
		level = t.GOMIPS
	case "ppc64", "ppc64le":
		level = t.GOPPC64
	case "riscv64":
		level = t.GORISCV64
	case "arm":
		goarm, float, _ := strings.Cut(t.GOARM, ",")
		switch t.FloatABI {
		case "":
		case "softfloat", "hardfloat":
			float = t.FloatABI
		default:
			// A Rust or Zig ABI replaces any float option in GOARM. A
			// GOARM=5 build is soft-float already, so it has no variant.
			if goarm == "5" {
				return ""
			}
			return VariantFromRustABI(t.FloatABI)
		}
		return VariantFromGo("arm", goarm+","+float)
	}
	return VariantFromGo(t.GOARCH, level)
}

// BundleName returns the release asset name of a project's bundle when
// several tools share the release of one repository: packslip.<subpath>.sigstore.json,
// with each "/" in the project's subpath within the repository replaced by "-".
// repo is the repository's own name, such as "github.com/acme/tools". The
// repository itself, or a project outside it, gets "packslip.sigstore.json".
// The repository is matched without regard to case, as the specification
// requires on forges ([Monorepos]).
//
// [Monorepos]: https://packslip.dev/release/v1/#monorepos
func BundleName(project, repo string) string {
	if repo != "" && len(project) > len(repo)+1 && project[len(repo)] == '/' && strings.EqualFold(project[:len(repo)], repo) {
		return "packslip." + strings.ReplaceAll(project[len(repo)+1:], "/", "-") + ".sigstore.json"
	}
	return "packslip.sigstore.json"
}
