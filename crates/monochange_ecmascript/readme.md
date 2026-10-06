# monochange_ecmascript

<!-- {=projectBrandLogo:"https://raw.githubusercontent.com/monochange/monochange/main/assets"} -->

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-dark-280.png">
  <img src="https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-280.png" alt="monochange" width="280" height="171">
</picture>

<!-- {/projectBrandLogo} -->

`monochange_ecmascript` provides shared JavaScript and TypeScript semantic-analysis helpers for `monochange` ecosystem adapters.

Use this crate when multiple adapters need the same ECMAScript module export extraction while keeping ecosystem-specific manifest logic in their own crates.
