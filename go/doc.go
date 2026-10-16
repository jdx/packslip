// Package packslip creates signed packslip release statements.
//
// It deliberately does not infer release metadata from archive names or
// implement a release tool adapter. Publishers describe their artifacts
// explicitly, call Build, then sign the resulting statement with SignKeyless.
package packslip
