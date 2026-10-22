// Package packslip creates signed packslip release statements.
//
// It deliberately does not infer release metadata from archive names or
// implement a release tool adapter. Publishers describe their artifacts
// explicitly, call Build, then sign the resulting statement with SignKeyless.
//
// In CI, AmbientIDToken obtains the OIDC token and IdentityFromToken derives
// the identity Fulcio will certify for it. AmbientIDToken is the only function
// that reads an OIDC token from the environment or requests one.
package packslip
