---
monochange: minor
---

Harden versioned file templates and expose release streams to command steps

- A `versioned_files` entry that declares both `value_template` and an explicit `format` + `fields` now writes the rendered value into those structured fields. Previously the template ignored `fields` and replaced the first SemVer-looking text in the file, which could stamp the wrong field or fail on files that carry no version (for example a digest manifest). Configure derived values directly:

  ```toml
  [[package.program.versioned_files]]
  path = "deploy/artifact.json"
  format = "json"
  fields = ["sha256"]
  value_template = "{{ artifact_digest }}"
  ```

- `monochange check` and `monochange step validate` now warn when one package, group, or ecosystem declares multiple `versioned_files` entries for the same path. Entries apply in declaration order, so divergent entries can overwrite each other's writes; the warning suggests merging the fields or splitting the values across files. Different owners sharing a file stay silent because that is legitimate.
- Command steps can gate on what a release rendered: `release.streams` and `release.outputs` list the changelog streams and named outputs of the prepared release, enabling conditions like `when = "{{ 'onchain' in release.streams }}"` for deployment workflows.
