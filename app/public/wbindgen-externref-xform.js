// wasm-bindgen 0.2.130 lowers `__wbindgen_externref_statics_end` to a constant
// when its externref transform runs. cargo-leptos 0.3.6 rewrites the main
// module's `reference-types` feature so that transform does not run on split
// bundles, and the generated glue still imports this module by a bare
// specifier. Browsers reject that specifier, so hydration never starts.
//
// The same glue refuses to free heap slots below 1028: undefined, null, true,
// and false occupy 1024..1027. This function is that boundary.
export function __wbindgen_externref_statics_end() {
	return 1028;
}
