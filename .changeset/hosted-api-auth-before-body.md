---
monochange_app: fix
---

# Check hosted release credentials before reading the request body

`POST /api/release-commits` and `POST /api/release-requests` now verify the `Authorization` bearer token (GitHub Actions OIDC or `MONOCHANGE_TOKEN`) before reading the request body. A caller without valid credentials gets `401` without the server buffering its payload. Previously the JSON body, up to 32 MiB for release commits, was read and parsed before authentication.

The release commit body limit drops from 32 MiB to 8 MiB, which still fits monochange's own grouped release (about 3 MiB). A body over the limit now returns `413 Payload Too Large` instead of `400`, and the release request endpoint keeps a 2 MiB limit. An OIDC token for one repository still gets `403` when the request targets another, now before the repository lookup.

`AppState` gains `oidc: Arc<OidcVerifier>`, so the GitHub Actions JWKS is cached across requests instead of being fetched on every call.
