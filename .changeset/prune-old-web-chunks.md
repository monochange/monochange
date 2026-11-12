---
monochange_app: fix
---

# Remove obsolete split modules from assembled web builds

The web asset plugin now uses each entry's split manifest to remove obsolete WASM chunks and their compressed copies from that entry's namespace. Incremental builds retain chunks still named by the deployed manifest; an invalid manifest stops assembly before pruning.
