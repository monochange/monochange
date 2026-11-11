import { initSync } from "/pkg/monochange_app.js";
        let sharedImports = undefined;
function getSharedImports() {
    if (sharedImports === undefined) {
        const mainExports = initSync(undefined, undefined);
        sharedImports = {
            __wasm_split: {
                __indirect_function_table: mainExports.__indirect_function_table,
                __stack_pointer: mainExports.__stack_pointer,
                memory: mainExports.memory,
            },
        };
    }
    return sharedImports;
}
function wrapAsyncCb(callee) {
    return async (callbackIndex, callbackData) => {
        let success;
        try {
            await callee();
            success = true;
        } catch (e) {
            console.error(e);
            success = false;
        } finally {
            const sharedImports = getSharedImports();
            sharedImports.__wasm_split.__indirect_function_table.get(callbackIndex)(callbackData, success);
        }
    }
}
function makeLoad(url, deps) {
    const loader = async () => {
        const parallelStuff = deps.map(d => d());
        const fetchSelf = fetch(url);
        parallelStuff.push(fetchSelf);
        await Promise.all(parallelStuff);
        const response = await fetchSelf;
        const imports = getSharedImports();
        return await WebAssembly.instantiateStreaming(response, imports);
    };
    let loadingModule = undefined;
    return () => {
        if (loadingModule === undefined) loadingModule = loader();
        return loadingModule;
    }
}
/* __book_route_view_17666993872358248534, __pricing_route_view_14841084612111332454 */
const __chunk_11 = makeLoad(new URL("./chunk_11.wasm", import.meta.url), []);
/* __book_route_view_17666993872358248534 */
const __chunk_12 = makeLoad(new URL("./chunk_12.wasm", import.meta.url), []);
export const __wasm_split_load___book_route_view_17666993872358248534 = wrapAsyncCb(makeLoad(new URL("./split___book_route_view_17666993872358248534.wasm", import.meta.url), [__chunk_11, __chunk_12]));
export const __wasm_split_load___pricing_route_view_14841084612111332454 = wrapAsyncCb(makeLoad(new URL("./split___pricing_route_view_14841084612111332454.wasm", import.meta.url), [__chunk_11]));
