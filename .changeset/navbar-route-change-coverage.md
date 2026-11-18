---
monochange_app: refactor
---

# Pin the navigation route-change collapse contract

The navbar's route-change effect now shares one `close_menus` collapse core with the Escape handler, and the effect is created only in the browser build, like `color_mode`'s: reactive effects compile out of the server build, so the served markup and client behavior are unchanged while the collapse contract itself is unit-testable on the server. The web navigation verification journey now also opens the mobile panel on the homepage, follows a section link from that panel, and asserts the client-side route change collapses it (`aria-expanded` back to `false` and the panel removed from the accessibility tree).

The repository's JavaScript lint gate is warning-free again without weakening any rule: the deferred hydration loader keeps its bare function-expression contract (documented where the lint exception is declared), and the sequential polling, shutdown, and per-route benchmark loops keep per-site exceptions naming the ordering dependency each one protects.
