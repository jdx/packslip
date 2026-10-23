package packslip

import "testing"

func TestArchFromGo(t *testing.T) {
	for _, tc := range []struct{ goarch, goarm, want string }{
		{"amd64", "", "x86_64"},
		{"arm64", "", "aarch64"},
		{"386", "", "i686"},
		{"arm", "", "armv6"},
		{"arm", "6", "armv6"},
		{"arm", "7", "armv7"},
		{"arm", "7,softfloat", "armv7"},
		{"arm", ",hardfloat", "armv6"},
		{"riscv64", "", "riscv64"},
		{"s390x", "", "s390x"},
		{"ppc64le", "", "powerpc64le"},
		{"ppc64", "", "powerpc64"},
		{"loong64", "", "loongarch64"},
		{"mipsle", "", "mipsel"},
		{"mips64le", "", "mips64el"},
		{"mips", "", "mips"},
		{"all", "", ""},
		{"wasm", "", "wasm"},
	} {
		if got := ArchFromGo(tc.goarch, tc.goarm); got != tc.want {
			t.Errorf("ArchFromGo(%q, %q) = %q, want %q", tc.goarch, tc.goarm, got, tc.want)
		}
	}
}

func TestVariantFromGo(t *testing.T) {
	for _, tc := range []struct{ goarch, level, want string }{
		{"amd64", "", ""},
		{"amd64", "v1", ""},
		{"amd64", "v3", "v3"},
		{"386", "sse2", ""},
		{"386", "softfloat", "softfloat"},
		{"arm64", "v8.0", ""},
		{"arm64", "v8.2", "v8.2"},
		{"arm64", "v8.2,lse", "v8.2-lse"},
		{"arm", "", ""},
		{"arm", "7", ""},
		{"arm", "7,hardfloat", ""},
		{"arm", "7,softfloat", "softfloat"},
		{"arm", "6,softfloat", "softfloat"},
		{"arm", "5,softfloat", ""},
		{"arm", "5", ""},
		{"arm", "5,hardfloat", "5-hardfloat"},
		{"mips", "hardfloat", ""},
		{"mipsle", "softfloat", "softfloat"},
		{"ppc64le", "power9", "power9"},
		{"ppc64", "power8", ""},
		{"riscv64", "rva23u64", "rva23u64"},
		{"s390x", "anything", ""},
		{"all", "", ""},
	} {
		if got := VariantFromGo(tc.goarch, tc.level); got != tc.want {
			t.Errorf("VariantFromGo(%q, %q) = %q, want %q", tc.goarch, tc.level, got, tc.want)
		}
	}
}

func TestVariantFromRustABI(t *testing.T) {
	for _, tc := range []struct{ abi, want string }{
		{"gnu", ""},
		{"musl", ""},
		{"gnueabihf", ""},
		{"musleabihf", ""},
		{"gnueabi", "softfloat"},
		{"gnueabi.2.17", "softfloat"},
		{"musleabi", "softfloat"},
		{"", ""},
	} {
		if got := VariantFromRustABI(tc.abi); got != tc.want {
			t.Errorf("VariantFromRustABI(%q) = %q, want %q", tc.abi, got, tc.want)
		}
	}
}

func TestNewBareBin(t *testing.T) {
	for _, tc := range []struct {
		name, format, command string
		want                  Bin
	}{
		{"tool.gz", "gz", "tool", Bin{Path: "tool", Name: "tool"}},
		{"tool-linux-x64.xz", "xz", "tool", Bin{Path: "tool-linux-x64", Name: "tool"}},
		{"tool-windows.exe", "raw", "tool.exe", Bin{Path: "tool-windows.exe", Name: "tool"}},
	} {
		if got := NewBareBin(tc.name, tc.format, tc.command); got != tc.want {
			t.Errorf("NewBareBin(%q, %q, %q) = %+v, want %+v", tc.name, tc.format, tc.command, got, tc.want)
		}
	}
}

func TestGoTarget(t *testing.T) {
	for _, tc := range []struct {
		target        GoTarget
		arch, variant string
	}{
		{GoTarget{GOARCH: "amd64", GOAMD64: "v3"}, "x86_64", "v3"},
		{GoTarget{GOARCH: "amd64", GOAMD64: "v1"}, "x86_64", ""},
		{GoTarget{GOARCH: "amd64", GOARM64: "v8.2"}, "x86_64", ""},
		{GoTarget{GOARCH: "386", GO386: "softfloat"}, "i686", "softfloat"},
		{GoTarget{GOARCH: "arm64", GOARM64: "v8.2,lse"}, "aarch64", "v8.2-lse"},
		{GoTarget{GOARCH: "mipsle", GOMIPS: "softfloat"}, "mipsel", "softfloat"},
		{GoTarget{GOARCH: "ppc64le", GOPPC64: "power9"}, "powerpc64le", "power9"},
		{GoTarget{GOARCH: "riscv64", GORISCV64: "rva23u64"}, "riscv64", "rva23u64"},
		{GoTarget{GOARCH: "arm", GOARM: "7"}, "armv7", ""},
		{GoTarget{GOARCH: "arm", GOARM: "7,softfloat"}, "armv7", "softfloat"},
		{GoTarget{GOARCH: "arm", GOARM: "7", FloatABI: "softfloat"}, "armv7", "softfloat"},
		{GoTarget{GOARCH: "arm", GOARM: "7", FloatABI: "hardfloat"}, "armv7", ""},
		{GoTarget{GOARCH: "arm", GOARM: "5", FloatABI: "hardfloat"}, "armv5", "5-hardfloat"},
		{GoTarget{GOARCH: "arm", GOARM: "5", FloatABI: "gnueabi"}, "armv5", ""},
		{GoTarget{GOARCH: "arm", GOARM: "7", FloatABI: "gnueabi"}, "armv7", "softfloat"},
		{GoTarget{GOARCH: "arm", GOARM: "7", FloatABI: "gnueabihf"}, "armv7", ""},
		{GoTarget{GOARCH: "arm", GOARM: "7,hardfloat", FloatABI: "gnueabi"}, "armv7", "softfloat"},
		{GoTarget{GOARCH: "arm", GOARM: "7,softfloat", FloatABI: "gnueabihf"}, "armv7", ""},
		{GoTarget{GOARCH: "all"}, "", ""},
	} {
		if got := tc.target.Arch(); got != tc.arch {
			t.Errorf("%+v.Arch() = %q, want %q", tc.target, got, tc.arch)
		}
		if got := tc.target.Variant(); got != tc.variant {
			t.Errorf("%+v.Variant() = %q, want %q", tc.target, got, tc.variant)
		}
	}
}

func TestBundleName(t *testing.T) {
	for _, tc := range []struct{ project, repo, want string }{
		{"github.com/acme/tools", "github.com/acme/tools", "packslip.sigstore.json"},
		{"github.com/acme/tools/oxlint", "github.com/acme/tools", "packslip.oxlint.sigstore.json"},
		{"github.com/acme/tools/crates/cli", "github.com/acme/tools", "packslip.crates-cli.sigstore.json"},
		{"github.com/Acme/Tools/oxlint", "github.com/acme/tools", "packslip.oxlint.sigstore.json"},
		{"github.com/acme/Tools/Crates/CLI", "github.com/acme/tools", "packslip.Crates-CLI.sigstore.json"},
		{"github.com/acme/tools/", "github.com/acme/tools", "packslip.sigstore.json"},
		{"github.com/acme/toolsmith", "github.com/acme/tools", "packslip.sigstore.json"},
		{"github.com/other/tools/oxlint", "github.com/acme/tools", "packslip.sigstore.json"},
		{"mytool.example.com", "github.com/acme/tools", "packslip.sigstore.json"},
		{"github.com/acme/tools/oxlint", "", "packslip.sigstore.json"},
	} {
		if got := BundleName(tc.project, tc.repo); got != tc.want {
			t.Errorf("BundleName(%q, %q) = %q, want %q", tc.project, tc.repo, got, tc.want)
		}
	}
}
