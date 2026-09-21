---
"monochange": minor
---

# Version your app with store build numbers and calendar labels

Repositories that ship an app alongside their libraries can now give the app its own delivery numbers without changing how anything else is versioned.

Two additions:

- **A build number per app.** Declare a counter in a file you own and commit, and monochange advances it on every release. This is the number the App Store and Google Play expect, and it is separate from the version your users see.
- **A calendar-style display version.** Render labels such as `2026.09.2` from the release date and how many times you have released that month.

The two settings that matter most:

- `reset = "version"` restarts the build number when the version changes. This matches the iOS App Store, where each new version can start again from build 1.
- `reset = "never"` never restarts. This matches Google Play, where a version code that has been used once can never be used again.

Set up looks like this:

```toml
[version_scheme.calver]
template = "{{ year }}.{{ month_padded }}.{{ release_of_month }}"

[package.app]
display_version = "calver"

[package.app.values.build]
file = "build.json"
field = "build"
on_release = "increment"
reset = "never"

[[package.app.versioned_files]]
path = "pubspec.yaml"
type = "dart"
value_template = "{{ identity }}+{{ build }}"
```

## What you have to do

Create the counter file yourself and commit it, with the number to start from:

```json
{ "build": 0 }
```

If your app is already published, put its current build number there instead of `0`, so the next release continues from where you are. If the file is missing, monochange stops with an error naming the file and field rather than guessing a starting point — a wrong guess would produce a build the stores reject as a duplicate.

Nothing changes for projects that do not configure any of this.
