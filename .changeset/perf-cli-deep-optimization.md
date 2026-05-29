---
monochange: patch
---

# Skip async initialization for --version flag

Previously, `mc --version` initialized the full Tokio runtime, rustls
crypto provider, and tracing subscriber before printing the version.
Now a synchronous fast path checks for --version/-V before any async
initialization, reducing latency from ~7ms to ~4ms.

Also removed the redundant rustls crypto provider installation from
run_cli_binary_from_env — it's already lazily installed by
build_http_client in monochange_hosting when an HTTP request is made.
