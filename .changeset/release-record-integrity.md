---
monochange: patch
---

# Verify the complete prepared release record

Reject modified release records before `CommitRelease`, even when their target IDs and versions still match the prepared release. Tag settings, package publication targets, provider metadata, changelogs, and release values must match the prepared manifest. A valid preparation timestamp and JSON key ordering remain unchanged. Rejected records leave overlapping release records untouched. If the expected record cannot be serialized, validation reports the cause before writing.

Previously, `monochange step commit-release` could accept a record with matching targets but modified release instructions. It now reports a content mismatch. `PrepareRelease` preserves existing records. Recovery requires restoring the record from a trusted preparation, or explicitly configuring `update_release_json = true` on `CommitRelease` to restore the expected record.
