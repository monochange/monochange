---
monochange_app: fix
---

# Share JSON decoding for projects resources

Projects route resources now decode their SSR JSON with the same byte-based decoder used by server functions. The JSON wire format remains unchanged, while duplicate WebAssembly decoders for the same response types can be eliminated.
