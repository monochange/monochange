package sdk

import "strings"

// Normalize removes surrounding whitespace from SDK output.
func Normalize(value string) string { return strings.TrimSpace(value) }
