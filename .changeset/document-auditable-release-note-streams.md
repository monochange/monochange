---
"@monochange/skill": minor
---

# teach agents to write auditable audience-specific changesets

The Monochange agent skill now explains how to separate developer and user release notes without adding audience metadata to changeset bodies. Agents select a configured type, keep one stream per file, and write prose for that stream's readers.

```markdown
---
app: app_feature
---

# make project lists open faster

Large project lists now appear sooner and remain responsive while more results load.
```

For mobile repositories, the guidance distinguishes a `native` major change that requires a new store binary from an `app_feature` minor change that may use a patch delivery system such as Shorebird. When one implementation matters to both developers and users, agents create two independently reviewable changesets with different types and audience-appropriate prose rather than combining both audiences in one file.
