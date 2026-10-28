---
monochange_publish: patch
---

# Probe registries concurrently while filtering pending publishes

Publish readiness and placeholder flows check every package against its registry before publishing, one HTTP round trip at a time. Those probes now run eight at a time, so a release with many packages spends the longest single registry round trip (plus a bounded tail) instead of the sum of every package's latency. Results keep the input order, external-mode packages stay unprobed, and errors surface exactly as before.
