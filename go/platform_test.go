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
