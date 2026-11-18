# GitHub body limits

- Preserve metadata and complete rendered entries when hosted release notes exceed 125,000 characters.
- Expose a configurable release cap and a reusable bounded-body API that reserves space for the continuation notice.
- Enforce the existing PR cap even when its header or changelog pointer alone is oversized.
- Cover exact limits, Unicode, complete entries, oversized single entries, and large PR headers; validate schema and affected packages.
