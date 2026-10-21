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
