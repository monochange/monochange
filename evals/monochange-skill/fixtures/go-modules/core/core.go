// Package core provides the parsing and validation primitives shared by the
// Acme services.
package core

import "strings"

// Document is a parsed configuration document.
type Document struct {
	// Source is the raw text the document was parsed from.
	Source string
}

// Parse builds a Document from raw source text.
func Parse(source string) Document {
	return Document{Source: source}
}

// LineCount reports the number of non-empty lines in the document.
func (document Document) LineCount() int {
	count := 0
	for _, line := range strings.Split(document.Source, "\n") {
		if strings.TrimSpace(line) != "" {
			count++
		}
	}
	return count
}

// Validate returns every problem found in the document.
func Validate(document Document) []string {
	if strings.TrimSpace(document.Source) == "" {
		return []string{"document is empty"}
	}
	return nil
}
