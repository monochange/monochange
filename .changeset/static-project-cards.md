---
monochange_app: fix
---

# Reduce shared project rendering code

`ProjectCard` shares a type-erased render boundary across the workspace and organization routes while retaining normal hydration. Both callers consume its view directly. Decorative arrow SVGs use Leptos' inert markup representation. Header and workspace resource reads also borrow their results before selecting the fields needed for display.
