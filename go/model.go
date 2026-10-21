package packslip

import (
	"crypto/sha256"
	"crypto/sha512"
	"encoding/json"
	"errors"
	"fmt"
	"regexp"
	"strings"
	"time"
)

const (
	StatementType        = "https://in-toto.io/Statement/v1"
	ReleasePredicateType = "https://packslip.dev/release/v1"
	DSSEPayloadType      = "application/vnd.in-toto+json"
)

var (
	sha256Hex = regexp.MustCompile(`^[0-9a-f]{64}$`)
	sha512Hex = regexp.MustCompile(`^[0-9a-f]{128}$`)
	token     = regexp.MustCompile(`^[a-z0-9][a-z0-9_.-]*$`)
	semver    = regexp.MustCompile(`^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-((?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$`)
)

type Statement struct {
	Type          string    `json:"_type"`
	Subject       []Subject `json:"subject"`
	PredicateType string    `json:"predicateType"`
	Predicate     Predicate `json:"predicate"`
}
type Subject struct {
	Name   string `json:"name"`
	Digest Digest `json:"digest"`
}
type Digest struct {
	SHA256 string `json:"sha256"`
	SHA512 string `json:"sha512,omitempty"`
}
type Predicate struct {
	Project     string                     `json:"project"`
	Version     string                     `json:"version"`
	PublishedAt time.Time                  `json:"published_at"`
	Source      *Source                    `json:"source,omitempty"`
	Artifacts   []Artifact                 `json:"artifacts"`
	Resources   []Resource                 `json:"resources,omitempty"`
	Identity    Identity                   `json:"identity"`
	AttestedBy  string                     `json:"attested_by,omitempty"`
	Evidence    []Evidence                 `json:"evidence,omitempty"`
	NotesURL    string                     `json:"notes_url,omitempty"`
	Extensions  map[string]json.RawMessage `json:"extensions,omitempty"`
}
type Source struct {
	Repo   string `json:"repo"`
	Commit string `json:"commit,omitempty"`
	Tag    string `json:"tag,omitempty"`
}
type Artifact struct {
	Name       string                     `json:"name"`
	OS         string                     `json:"os,omitempty"`
	Arch       string                     `json:"arch,omitempty"`
	Libc       string                     `json:"libc,omitempty"`
	Variant    string                     `json:"variant,omitempty"`
	Size       int64                      `json:"size"`
	URL        string                     `json:"url,omitempty"`
	Format     string                     `json:"format"`
	Bin        []Bin                      `json:"bin,omitempty"`
	Requires   *Requires                  `json:"requires,omitempty"`
	Provenance []string                   `json:"provenance,omitempty"`
	Extensions map[string]json.RawMessage `json:"extensions,omitempty"`
}

// Bin is an executable in an artifact. JSON accepts the compact string form
// when Name is the file name (without a Windows .exe suffix).
type Bin struct {
	Path string
	Name string
}

func NewBin(p string) Bin      { return Bin{Path: p, Name: commandName(fileName(p))} }
func NamedBin(p, n string) Bin { return Bin{Path: p, Name: commandName(n)} }
func (b Bin) MarshalJSON() ([]byte, error) {
	if b.Name == commandName(fileName(b.Path)) {
		return json.Marshal(b.Path)
	}
	return json.Marshal(struct {
		Path string `json:"path"`
		Name string `json:"name"`
	}{b.Path, b.Name})
}
func (b *Bin) UnmarshalJSON(v []byte) error {
	var p string
	if json.Unmarshal(v, &p) == nil {
		*b = NewBin(p)
		return nil
	}
	var x struct {
		Path string `json:"path"`
		Name string `json:"name"`
	}
	if err := json.Unmarshal(v, &x); err != nil {
		return err
	}
	*b = NamedBin(x.Path, x.Name)
	return nil
}

type Requires struct {
	OSMin    string        `json:"os_min,omitempty"`
	GlibcMin string        `json:"glibc_min,omitempty"`
	Libs     *[]string     `json:"libs,omitempty"`
	Bin      []RequiredBin `json:"bin,omitempty"`
}
type RequiredBin struct {
	Name string `json:"name"`
	Min  string `json:"min,omitempty"`
}
type Resource struct {
	Kind       string                     `json:"kind"`
	Artifact   string                     `json:"artifact,omitempty"`
	OS         string                     `json:"os,omitempty"`
	Arch       string                     `json:"arch,omitempty"`
	Libc       string                     `json:"libc,omitempty"`
	Shell      string                     `json:"shell,omitempty"`
	Shells     []string                   `json:"shells,omitempty"`
	Name       string                     `json:"name,omitempty"`
	Bin        string                     `json:"bin,omitempty"`
	Format     string                     `json:"format,omitempty"`
	Archive    string                     `json:"archive,omitempty"`
	Asset      string                     `json:"asset,omitempty"`
	URL        string                     `json:"url,omitempty"`
	Repo       string                     `json:"repo,omitempty"`
	Exec       []string                   `json:"exec,omitempty"`
	Env        map[string]string          `json:"env,omitempty"`
	Extensions map[string]json.RawMessage `json:"extensions,omitempty"`
}
type Identity struct {
	Scheme      string `json:"scheme"`
	KeyID       string `json:"key_id"`
	Issuer      string `json:"issuer,omitempty"`
	PinWorkflow *bool  `json:"pin_workflow,omitempty"`
}
type Evidence struct {
	Kind   string `json:"kind"`
	Detail string `json:"detail,omitempty"`
}

// Validate checks the release-v1 structural invariants. It does not prove the
// signature; parsed and structurally valid input is not cryptographically verified.
func (s Statement) Validate() error {
	if s.Type != StatementType {
		return fmt.Errorf("packslip: _type must be %q", StatementType)
	}
	if s.PredicateType != ReleasePredicateType {
		return fmt.Errorf("packslip: predicateType must be %q", ReleasePredicateType)
	}
	p := s.Predicate
	if !validProject(p.Project) {
		return errors.New("packslip: invalid project")
	}
	if !semver.MatchString(p.Version) {
		return fmt.Errorf("packslip: invalid version %q", p.Version)
	}
	if p.PublishedAt.IsZero() {
		return errors.New("packslip: published_at must be RFC 3339")
	}
	if p.Identity.KeyID == "" || (p.Identity.Scheme != "sigstore-oidc" && p.Identity.Scheme != "sigstore-key") || (p.Identity.Scheme == "sigstore-oidc" && p.Identity.Issuer == "") {
		return errors.New("packslip: invalid signing identity")
	}
	if p.AttestedBy != "" && p.AttestedBy != "vendor" && p.AttestedBy != "repackager" {
		return errors.New("packslip: attested_by must be vendor or repackager")
	}
	if len(p.Artifacts) == 0 {
		return errors.New("packslip: a release needs at least one artifact")
	}
	if err := extensions("release", p.Extensions); err != nil {
		return err
	}
	subjects := map[string]Digest{}
	for _, x := range s.Subject {
		if _, ok := subjects[x.Name]; ok {
			return fmt.Errorf("packslip: duplicate subject %q", x.Name)
		}
		if !sha256Hex.MatchString(x.Digest.SHA256) || x.Digest.SHA512 != "" && !sha512Hex.MatchString(x.Digest.SHA512) {
			return fmt.Errorf("packslip: invalid digest for %q", x.Name)
		}
		subjects[x.Name] = x.Digest
	}
	artifacts := map[string]Artifact{}
	binNames := map[string]bool{}
	for _, a := range p.Artifacts {
		if _, ok := artifacts[a.Name]; ok {
			return fmt.Errorf("packslip: duplicate artifact %q", a.Name)
		}
		if _, ok := subjects[a.Name]; !ok {
			return fmt.Errorf("packslip: artifact %q has no subject", a.Name)
		}
		if a.Format == "" || !token.MatchString(a.Format) {
			return fmt.Errorf("packslip: invalid artifact format %q", a.Name)
		}
		if a.Size < 0 {
			return fmt.Errorf("packslip: invalid artifact size %q", a.Name)
		}
		if err := extensions("artifact", a.Extensions); err != nil {
			return err
		}
		for _, f := range []string{a.OS, a.Arch, a.Libc, a.Variant} {
			if f != "" && !token.MatchString(f) {
				return fmt.Errorf("packslip: invalid artifact token %q", f)
			}
		}
		bare := bareFormat(a.Format)
		for _, b := range a.Bin {
			if b.Path == "" || b.Name == "" || strings.Contains(b.Name, "/") || strings.HasSuffix(strings.ToLower(b.Name), ".exe") {
				return fmt.Errorf("packslip: invalid bin in %q", a.Name)
			}
			if bare && b.Path != bareFile(a.Name, a.Format) {
				return fmt.Errorf("packslip: bare artifact %q has wrong bin path", a.Name)
			}
			binNames[commandName(b.Name)] = true
		}
		if a.Requires != nil {
			if err := validateRequires(a.Name, *a.Requires); err != nil {
				return err
			}
		}
		for _, other := range artifacts {
			if selectorsEqual(a, other) {
				return fmt.Errorf("packslip: ambiguous artifacts %q and %q", a.Name, other.Name)
			}
		}
		artifacts[a.Name] = a
	}
	for _, a := range p.Artifacts {
		if a.Requires != nil {
			for _, b := range a.Requires.Bin {
				if binNames[commandName(b.Name)] {
					return fmt.Errorf("packslip: artifact %q requires its own bin", a.Name)
				}
			}
		}
	}
	assets := map[string]bool{}
	for i, r := range p.Resources {
		if err := validateResource(r, artifacts, subjects, binNames, p.Source); err != nil {
			return fmt.Errorf("packslip: resource %d: %w", i, err)
		}
		if r.Asset != "" {
			assets[r.Asset] = true
		}
	}
	for n := range subjects {
		if _, ok := artifacts[n]; !ok && !assets[n] {
			return fmt.Errorf("packslip: subject %q has no artifact or asset", n)
		}
	}
	return nil
}
func validateRequires(artifact string, r Requires) error {
	seen := map[string]bool{}
	if r.Libs != nil {
		for _, v := range *r.Libs {
			if !validLib(v) || seen[v] {
				return fmt.Errorf("packslip: invalid required library %q", v)
			}
			seen[v] = true
		}
	}
	seen = map[string]bool{}
	for _, b := range r.Bin {
		if !validRequiredBin(b.Name) || b.Min != "" && !validMin(b.Min) || seen[b.Name] {
			return fmt.Errorf("packslip: invalid required bin %q", b.Name)
		}
		seen[b.Name] = true
	}
	return nil
}
func validateResource(r Resource, arts map[string]Artifact, subjects map[string]Digest, bins map[string]bool, source *Source) error {
	if r.Kind == "" {
		return errors.New("empty kind")
	}
	if err := extensions("resource", r.Extensions); err != nil {
		return err
	}
	for _, x := range []string{r.OS, r.Arch, r.Libc} {
		if x != "" && !token.MatchString(x) {
			return errors.New("invalid selector")
		}
	}
	sources := 0
	for _, ok := range []bool{r.Archive != "", r.Asset != "", r.Repo != "", len(r.Exec) > 0} {
		if ok {
			sources++
		}
	}
	if sources != 1 {
		return errors.New("must specify exactly one of archive, asset, repo, exec")
	}
	var selected []Artifact
	for _, a := range arts {
		if fits(r, a) {
			selected = append(selected, a)
		}
	}
	if r.Artifact != "" && len(selected) == 0 {
		return fmt.Errorf("unknown or incompatible artifact %q", r.Artifact)
	}
	if r.Archive != "" {
		if !validRelativePath(r.Archive) {
			return errors.New("invalid archive path")
		}
		for _, a := range selected {
			if bareFormat(a.Format) {
				return errors.New("archive path in bare artifact")
			}
		}
	}
	if r.Asset != "" {
		if _, ok := subjects[r.Asset]; !ok {
			return fmt.Errorf("asset %q is not a subject", r.Asset)
		}
	}
	if r.Repo != "" {
		if !validRelativePath(r.Repo) || source == nil || source.Commit == "" {
			return errors.New("repo source needs valid path and source.commit")
		}
	}
	if len(r.Exec) > 0 && !bins[commandName(r.Exec[0])] {
		return fmt.Errorf("exec program %q is not declared", r.Exec[0])
	}
	if len(r.Env) > 0 && len(r.Exec) == 0 {
		return errors.New("env requires exec")
	}
	for n := range r.Env {
		if n == "" || strings.ContainsAny(n, "=\x00") {
			return fmt.Errorf("invalid environment name %q", n)
		}
	}
	if r.URL != "" && r.Asset == "" {
		return errors.New("url requires asset")
	}
	if r.Kind == "completion" || r.Kind == "man" {
		if r.Bin != "" && !bins[commandName(r.Bin)] {
			return errors.New("unknown resource bin")
		}
		if r.Bin == "" && len(bins) > 1 {
			return errors.New("resource needs bin")
		}
	}
	if r.Kind == "completion" {
		if (r.Shell != "") == (len(r.Shells) > 0) {
			return errors.New("completion needs exactly one of shell or shells")
		}
		shells := append([]string{}, r.Shells...)
		if r.Shell != "" {
			shells = append(shells, r.Shell)
		}
		for _, x := range shells {
			if !validShell(x) {
				return errors.New("invalid shell")
			}
		}
		mentions := false
		for _, x := range r.Exec {
			mentions = mentions || strings.Contains(x, "{shell}")
		}
		for _, x := range r.Env {
			mentions = mentions || strings.Contains(x, "{shell}")
		}
		if (len(r.Exec) > 0 && r.Shell != "") || (len(r.Shells) > 0 && (len(r.Exec) == 0 || !mentions)) {
			return errors.New("completion shell declaration incompatible with source")
		}
	}
	if r.Kind == "cli-spec" && (r.Format == "" || r.Bin == "" || !bins[commandName(r.Bin)]) {
		return errors.New("cli-spec needs format and declared bin")
	}
	if r.Kind == "skill" && (r.Name == "" || strings.Contains(r.Name, "/")) {
		return errors.New("skill needs name")
	}
	if r.Kind == "app" && r.Archive == "" {
		return errors.New("app must come from archive")
	}
	if r.Kind == "sbom" && (r.Format == "" || len(r.Exec) > 0) {
		return errors.New("sbom needs format and static source")
	}
	return nil
}
func extensions(owner string, e map[string]json.RawMessage) error {
	if _, ok := e[""]; ok {
		return fmt.Errorf("packslip: %s has empty extension key", owner)
	}
	return nil
}
func selectorsEqual(a, b Artifact) bool {
	return a.OS == b.OS && a.Arch == b.Arch && a.Libc == b.Libc && a.Variant == b.Variant && a.Format == b.Format
}
func fits(r Resource, a Artifact) bool {
	return (r.Artifact == "" || r.Artifact == a.Name) && (r.OS == "" || r.OS == a.OS) && (r.Arch == "" || r.Arch == a.Arch) && (r.Libc == "" || r.Libc == a.Libc) && !(r.Archive != "" && bareFormat(a.Format))
}
func bareFormat(f string) bool {
	return f == "raw" || f == "gz" || f == "xz" || f == "zst" || f == "bz2"
}
func bareFile(n, f string) string {
	if f == "raw" {
		return n
	}
	return strings.TrimSuffix(n, "."+f)
}
func validProject(v string) bool {
	p := strings.Split(v, "/")
	h := p[0]
	if !strings.Contains(h, ".") || strings.HasPrefix(h, ".") || strings.HasSuffix(h, ".") || strings.Contains(h, "..") {
		return false
	}
	for _, x := range strings.Split(h, ".") {
		if x == "" || strings.HasPrefix(x, "-") || strings.HasSuffix(x, "-") {
			return false
		}
		for _, c := range x {
			if !(c >= 'a' && c <= 'z' || c >= '0' && c <= '9' || c == '-') {
				return false
			}
		}
	}
	for _, x := range p[1:] {
		if x == "" || x == "." || x == ".." {
			return false
		}
		for _, c := range x {
			if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || strings.ContainsRune("._-~", c)) {
				return false
			}
		}
	}
	return true
}
func validRelativePath(v string) bool {
	return v != "" && !strings.HasPrefix(v, "/") && func() bool {
		for _, x := range strings.Split(v, "/") {
			if x == "" || x == ".." {
				return false
			}
		}
		return true
	}()
}
func validLib(v string) bool { return v != "" && !strings.ContainsAny(v, "/\\ \t\r\n") }
func validRequiredBin(v string) bool {
	return validLib(v) && !strings.HasSuffix(strings.ToLower(v), ".exe")
}
func validMin(v string) bool {
	return v != "" && v[0] >= '0' && v[0] <= '9' && regexp.MustCompile(`^[A-Za-z0-9.+-]+$`).MatchString(v)
}
func validShell(v string) bool { return v != "" && regexp.MustCompile(`^[a-z0-9_-]+$`).MatchString(v) }

// fileName is the last path segment, as the Rust crate reads it: empty for a
// path ending in "/", which Validate then refuses as an empty name.
func fileName(v string) string { return v[strings.LastIndex(v, "/")+1:] }
func commandName(v string) string {
	if len(v) >= 4 && strings.EqualFold(v[len(v)-4:], ".exe") {
		return v[:len(v)-4]
	}
	return v
}
func (s Statement) CanonicalJSON() ([]byte, error) {
	if err := s.Validate(); err != nil {
		return nil, err
	}
	return json.Marshal(s)
}
func EqualDigests(data []byte) Digest {
	return Digest{SHA256: fmt.Sprintf("%x", sha256.Sum256(data)), SHA512: fmt.Sprintf("%x", sha512.Sum512(data))}
}
