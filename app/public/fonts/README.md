# Website fonts

Self-hosted static fonts from Google Fonts, used under the accompanying SIL Open Font License files:

- Bricolage Grotesque, weight 800: display headings.
- Hanken Grotesk, weights 400 and 700: body and controls.

Sources: https://fonts.google.com/specimen/Bricolage+Grotesque and https://fonts.google.com/specimen/Hanken+Grotesk. Self-hosting keeps page rendering independent of third-party font requests.

The web build uses `pyftsubset` with Brotli support to generate WOFF2 subsets from these source files. The subsets cover Latin, combining marks, common punctuation and arrows; browsers use system fallback fonts for other characters. The generated files are build artifacts, and the source TTFs are not copied into the deployed site.
