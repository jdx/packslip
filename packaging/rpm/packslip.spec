%global debug_package %{nil}
%global rustflags_codegen_units 1
%global rustflags_debuginfo 0

Name: packslip
Version: @VERSION@
Release: 1%{?dist}
Summary: Install authenticated upstream releases
License: MIT
URL: https://packslip.dev
Source0: packslip-%{version}.tar.gz
ExclusiveArch: x86_64 aarch64
BuildRequires: rust >= 1.93
BuildRequires: cargo
BuildRequires: gcc
BuildRequires: cmake
BuildRequires: pkgconf-pkg-config
Requires: ca-certificates

%description
Packslip verifies signed release metadata and installs a complete upstream
release tree, exporting only its declared commands. It preserves trust and
ownership state without running downloaded code or editing shell setup.

%prep
%autosetup

%build
export CARGO_HOME="$PWD/.cargo-home"
export CARGO_TARGET_DIR="$PWD/target"
# Fedora's hardening linker flags also apply to aws-lc's compiler probes.
export LDFLAGS="${LDFLAGS:-} -fPIE"
cargo build --release --frozen --no-default-features --features install-cli --bin packslip

%check
export CARGO_HOME="$PWD/.cargo-home"
export CARGO_TARGET_DIR="$PWD/target"
cargo test --frozen --no-default-features --features install-cli

%install
install -Dm755 target/release/packslip %{buildroot}%{_bindir}/packslip
install -d %{buildroot}%{_sysconfdir}/packslip/pins.d
install -Dm644 packaging/packslip.1 %{buildroot}%{_mandir}/man1/packslip.1
install -d %{buildroot}%{_datadir}/bash-completion/completions
target/release/packslip completion bash > %{buildroot}%{_datadir}/bash-completion/completions/packslip
install -d %{buildroot}%{_datadir}/zsh/site-functions
target/release/packslip completion zsh > %{buildroot}%{_datadir}/zsh/site-functions/_packslip
install -d %{buildroot}%{_datadir}/fish/vendor_completions.d
target/release/packslip completion fish > %{buildroot}%{_datadir}/fish/vendor_completions.d/packslip.fish

%files
%license LICENSE
%doc README.md content/docs/bootstrap.md
%{_bindir}/packslip
%{_mandir}/man1/packslip.1*
%dir %{_sysconfdir}/packslip
%dir %{_sysconfdir}/packslip/pins.d
%{_datadir}/bash-completion/completions/packslip
%{_datadir}/zsh/site-functions/_packslip
%{_datadir}/fish/vendor_completions.d/packslip.fish
