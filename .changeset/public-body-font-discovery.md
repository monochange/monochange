---
monochange_app: fix
---

# Discover body fonts in the server-rendered head

Both browser entries preload the existing Hanken 400 and 700 WOFF2 files with anonymous CORS. Browsers can request body and navigation fonts before the shared stylesheet finishes loading, shortening the HTML-to-stylesheet-to-font discovery chain. Deploy the updated server alongside the existing font assets at `/fonts/hanken-400.woff2` and `/fonts/hanken-700.woff2`.
