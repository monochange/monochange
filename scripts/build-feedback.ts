import { build } from "tsdown";
import { copyFile, mkdir, readFile } from "node:fs/promises";
import { gzipSync } from "node:zlib";

await build({
	config: false,
	entry: {
		index: "packages/monochange__feedback/src/index.ts",
		client: "packages/monochange__feedback/src/client.ts",
		widget: "packages/monochange__feedback/src/widget.ts",
	},
	outDir: "packages/monochange__feedback/dist",
	target: "es2022",
	platform: "browser",
	format: "esm",
	dts: true,
	clean: true,
});
await build({
	config: false,
	entry: { feedback: "packages/monochange__feedback/src/index.ts" },
	outDir: "packages/monochange__feedback/dist/embed",
	target: "es2022",
	platform: "browser",
	format: "iife",
	globalName: "monochangeFeedback",
	minify: true,
	dts: false,
});
await build({
	config: false,
	entry: { index: "packages/monochange__feedback-react-native/src/index.tsx" },
	outDir: "packages/monochange__feedback-react-native/dist",
	target: "es2022",
	platform: "neutral",
	format: "esm",
	deps: {
		neverBundle: [/^react(?:\/|$)/, /^react-native(?:\/|$)/, "@monochange/feedback/client"],
		onlyBundle: false,
	},
	dts: true,
	clean: true,
});
await mkdir("app/public/embed", { recursive: true });
await copyFile(
	"packages/monochange__feedback/dist/embed/feedback.iife.js",
	"app/public/embed/feedback.js",
);
const browserBundle = await readFile("app/public/embed/feedback.js");
const browserBundleBudget = { raw: 32_000, gzip: 10_000 };
if (
	browserBundle.length > browserBundleBudget.raw ||
	gzipSync(browserBundle).length > browserBundleBudget.gzip
) {
	throw new Error("The feedback browser bundle exceeds its 32 KB raw / 10 KB gzip budget");
}
await build({
	config: false,
	entry: { worker: "packages/monochange__feedback-worker/src/worker.ts" },
	outDir: "packages/monochange__feedback-worker/dist",
	target: "node22",
	platform: "node",
	format: "esm",
	dts: false,
	clean: true,
});
