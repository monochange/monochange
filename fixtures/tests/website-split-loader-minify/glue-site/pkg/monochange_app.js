import * as import1 from "./__wasm_split.______________________.js";

let wasm;
let wasmMemory;

function __wbg_init(module_or_path) {
	return Promise.resolve().then(() => {
		const imports = {};
		imports["./__wasm_split.______________________.js"] = import1;
		return fetch(module_or_path).then((response) => {
			return WebAssembly.instantiateStreaming(response, imports).then((obj) => {
				wasm = obj.instance.exports;
				wasmMemory = wasm.memory;
				wasm.__wbindgen_start();
			});
		});
	});
}

export function hydrate() {
	console.log("hydrate the app");
}

export function initSync(module) {
	const instance = new WebAssembly.Instance(module, {});
	wasm = instance.exports;
	wasmMemory = wasm.memory;
	wasm.__wbindgen_start();
}

class IntoUnderlyingByteSource {
	constructor() {
		this.established = true;
	}

	pull(controller) {
		controller.close();
	}
}

function helperUnusedByExports(value) {
	return new IntoUnderlyingByteSource();
}

export { __wbg_init as default };
