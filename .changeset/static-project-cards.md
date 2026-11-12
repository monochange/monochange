---
monochange_app: fix
---

# Reduce shared project-card rendering code

`ProjectCard` renders its snapshot content through the framework's HTML escaping and retains its native link instead of hydrating an asynchronous child tree. Its shared return type is `AnyView`; both callers consume that view directly. Keep WASM handlers and reactive descendants outside the serialized snapshot. Header and workspace resource reads also borrow their results before selecting the fields needed for display.
