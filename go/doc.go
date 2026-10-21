// Package packslip creates signed packslip release statements.
//
// It deliberately does not infer release metadata from archive names or
// implement a release tool adapter. Publishers describe their artifacts
// explicitly, call Build, then sign the resulting statement with SignKeyless.
//
// In CI, AmbientIDToken and IdentityFromToken obtain the OIDC token and the
// identity Fulcio will certify for it; they are the only functions that read
// the environment or make an OIDC request.
package packslip
