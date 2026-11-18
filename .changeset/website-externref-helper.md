---
monochange_app: fix
---

# Serve the wasm-bindgen externref helper before hydration

cargo-leptos 0.3.6 hides the `reference-types` feature so wasm-bindgen will not rewrite split modules. wasm-bindgen 0.2.130 still emits this import in `monochange_app.js`:

```js
import * as __wbindgen_externref_xform__ from "__wbindgen_externref_xform__";
```

Browsers reject that bare specifier, so `hydrate()` never runs. The document shell now declares an import map before the hydration module:

```html
<script type="importmap">
	{"imports":{"__wbindgen_externref_xform__":"/wbindgen-externref-xform.js"}}
</script>
```

`app/public/wbindgen-externref-xform.js` exports `__wbindgen_externref_statics_end` as `1028`, the same boundary the generated glue uses when it leaves the four reserved externref slots (indices 1024 through 1027) alone. A site build must keep serving that file from the site root.
