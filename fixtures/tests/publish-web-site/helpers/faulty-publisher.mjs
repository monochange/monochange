#!/usr/bin/env node
// Runs the real publisher in its own process with operating-system-style
// faults at chosen renames, so a test can interleave two publishers or
// interrupt one at an exact step. The faults wrap node:fs before the
// publisher module loads (module.syncBuiltinESMExports updates the named
// ESM bindings the publisher imports); the publisher carries no test hooks.
//
// argv: <publisher.ts> <plan JSON>
//
// plan.run is { "publish": { "webRoot", "siteRoot" } }.
// plan.faults lists { "from"?, "to", "effect", "barrier"? } with absolute
// paths. A fault matches a renameSync(from, to) call by exact `to` and, when
// given, exact `from`; each fault fires at most once.
//   "fail"  throws an EIO error instead of renaming.
//   "pause" renames, writes `<barrier>.paused`, then blocks until
//           `<barrier>.resume` exists. The block is bounded: the helper
//           exits 3 if no resume arrives, so a broken test cannot leak it.
//
// Prints the publisher's result as JSON on success; prints the failure
// message to stderr and exits 1 on a publisher failure.
import fs from "node:fs";
import { syncBuiltinESMExports } from "node:module";
import { pathToFileURL } from "node:url";

const PAUSE_LIMIT_MS = 15_000;

const [publisher, planJson] = process.argv.slice(2);
const plan = JSON.parse(planJson);
const pending = [...plan.faults];
const realRenameSync = fs.renameSync;

function takeFault(from, to) {
	const index = pending.findIndex(
		(fault) => fault.to === String(to) && (fault.from === undefined || fault.from === String(from)),
	);

	return index === -1 ? undefined : pending.splice(index, 1)[0];
}

function pause(barrier) {
	fs.writeFileSync(`${barrier}.paused`, `${process.pid}\n`);
	const sleeper = new Int32Array(new SharedArrayBuffer(4));
	const deadline = Date.now() + PAUSE_LIMIT_MS;
	while (!fs.existsSync(`${barrier}.resume`)) {
		if (Date.now() > deadline) {
			console.error(`faulty-publisher: no resume for ${barrier}`);
			process.exit(3);
		}
		Atomics.wait(sleeper, 0, 0, 10);
	}
}

fs.renameSync = (from, to) => {
	const fault = takeFault(from, to);
	if (fault?.effect === "fail") {
		const error = new Error(`EIO: injected i/o error, rename '${from}' -> '${to}'`);
		error.code = "EIO";
		throw error;
	}

	realRenameSync(from, to);
	if (fault?.effect === "pause") pause(fault.barrier);
};
syncBuiltinESMExports();

const { publishSite } = await import(pathToFileURL(publisher).href);
try {
	const result = publishSite(plan.run.publish.webRoot, plan.run.publish.siteRoot);
	process.stdout.write(`${JSON.stringify(result)}\n`);
} catch (error) {
	process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
	process.exitCode = 1;
}
