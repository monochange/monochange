---
monochange_core: major
monochange_publish: major
---

# Add native ecosystem fields to Rust configuration and endpoint structs

> Rust callers constructing `WorkspaceConfiguration` or `RegistryEndpoints` with struct literals must initialize the new fields.

`WorkspaceConfiguration` now carries `swift`, `cocoapods`, and `gradle` settings. Add these entries to an existing struct literal:

```rust
swift: EcosystemSettings::default(),
cocoapods: EcosystemSettings::default(),
gradle: EcosystemSettings::default(),
```

Previously serialized workspace configurations remain readable: omitted native ecosystem settings deserialize to their defaults.

For `RegistryEndpoints`, prefer `RegistryEndpoints::from_env()` and override the endpoints needed by your caller. Existing explicit struct literals need these additional fields:

```rust
cocoapods_trunk_api: "https://trunk.cocoapods.org/api/v1".to_string(),
maven_central: "https://repo1.maven.org/maven2".to_string(),
```

The built-in registry defaults remain available when the corresponding environment overrides are absent.
