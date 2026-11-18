---
monochange_app: fix
---

# Share the stylesheet between web entries

The website and projects entries now load `/pkg/site/monochange_app.css`. Builds compile it from both frontends' source files and remove the obsolete projects stylesheet and its compressed copies from the assembled site.
