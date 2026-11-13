import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";

const FEATURES = [
	"--enable-bulk-memory",
	"--enable-nontrapping-float-to-int",
	"--enable-sign-ext",
	"--enable-mutable-globals",
	"--enable-reference-types",
	"--enable-multivalue",
];
const STRIP_METADATA = ["--strip-producers", "--strip-target-features"];
const PASSES = [
	STRIP_METADATA,
	["--flatten", "--rereloop", "-Oz", "--merge-similar-functions", "-Oz", ...STRIP_METADATA],
	["--flatten", "--rereloop", "-O3", ...STRIP_METADATA],
];

function compressedSizes(bytes: Buffer): [gzip: number, gzip9: number, brotli: number] {
	return [
		gzipSync(bytes).length,
		gzipSync(bytes, { level: 9 }).length,
		brotliCompressSync(bytes, {
			params: {
				[constants.BROTLI_PARAM_QUALITY]: 11,
				[constants.BROTLI_PARAM_LGWIN]: 24,
			},
		}).length,
	];
}

function moduleInterface(bytes: Buffer): string {
	const module = new WebAssembly.Module(new Uint8Array(bytes));

	return JSON.stringify({
		imports: WebAssembly.Module.imports(module)
			.map((entry) => JSON.stringify(entry))
			.toSorted(),
		exports: WebAssembly.Module.exports(module)
			.map((entry) => JSON.stringify(entry))
			.toSorted(),
	});
}

/** Re-optimize split modules without changing their linking contracts or increasing transfer sizes. */
export function optimizeWebWasm(pkgDir: string): { files: number; gzipSaved: number } {
	const scratch = mkdtempSync(join(tmpdir(), "monochange-wasm-"));
	let files = 0;
	let gzipSaved = 0;

	try {
		for (const name of readdirSync(pkgDir).filter((entry) => entry.endsWith(".wasm"))) {
			const input = join(pkgDir, name);
			const original = readFileSync(input);
			const contract = moduleInterface(original);
			const originalSizes = compressedSizes(original);
			let best = original;
			let bestSizes = originalSizes;

			for (const passes of PASSES) {
				const output = join(scratch, name);
				rmSync(output, { force: true });
				const result = spawnSync(
					process.env.MONOCHANGE_WASM_OPT_BIN ?? "wasm-opt",
					[...FEATURES, ...passes, input, "-o", output],
					{ encoding: "utf8" },
				);

				if (result.error || result.status !== 0) {
					throw new Error(`wasm-opt failed for ${name}: ${result.error?.message ?? result.stderr}`);
				}

				const candidate = readFileSync(output);

				if (moduleInterface(candidate) !== contract) {
					throw new Error(`wasm-opt changed imports or exports for ${name}`);
				}

				// Raw size alone is a poor proxy for transfer size. Keep a candidate
				// only when it improves the gate's gzip and neither deployed format grows.
				const sizes = compressedSizes(candidate);

				if (sizes[0] < bestSizes[0] && sizes.every((size, index) => size <= originalSizes[index])) {
					best = candidate;
					bestSizes = sizes;
				}
			}

			if (best !== original) {
				writeFileSync(input, best);
				files += 1;
				gzipSaved += originalSizes[0] - bestSizes[0];
			}
		}
	} finally {
		rmSync(scratch, { recursive: true, force: true });
	}

	return { files, gzipSaved };
}
