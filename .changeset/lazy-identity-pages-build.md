---
monochange_app: fix
---

# Split the browser identity routes

Login, connect, account, and OAuth callback now use separate lazy routes. The bundle-size check compares the callback chunk with the original callback baseline by route name, while the total budget continues to include all four chunks.
