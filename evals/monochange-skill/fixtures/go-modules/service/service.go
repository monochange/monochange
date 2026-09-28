// Package service loads configuration documents through the Acme core parser
// and reports their status.
package service

import (
	"fmt"

	"github.com/acme/core"
)

// Load parses source text under the given name and returns a status line.
func Load(name string, source string) (string, error) {
	document := core.Parse(source)
	if problems := core.Validate(document); len(problems) > 0 {
		return "", fmt.Errorf("%s: %s", name, problems[0])
	}
	return fmt.Sprintf("%s: %d lines", name, document.LineCount()), nil
}
