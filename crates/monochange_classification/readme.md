# monochange_classification

Compatibility findings and the report contract for [`monochange change classify`](https://monochange.github.io/monochange/).

This crate owns the published classification schema. Its `SCHEMA_VERSION` is independent of the monochange release train: it advances only when the classification report contract changes, and every version has a frozen JSON Schema plus deterministic artifact fixtures under `schemas/`.

`monochange change classify` renders the report defined here; agents and CI automations parse it with the matching `classification.v<version>.schema.json`.
