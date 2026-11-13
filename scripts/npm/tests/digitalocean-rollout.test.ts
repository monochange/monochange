import assert from "node:assert/strict";
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import {
	chmodSync,
	chownSync,
	copyFileSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	readdirSync,
	rmSync,
	utimesSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, test } from "vitest";

const repo = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const fixtures = join(repo, "fixtures/tests/digitalocean-rollout");
const scripts = process.env.ROLLOUT_SCRIPT_ROOT ?? join(repo, "app/deploy/digitalocean");
const image = `monochange-app:${"a".repeat(40)}`;
const owned: string[] = [];
const children: { child: ChildProcess; closed: Promise<void> }[] = [];
const linux =
	process.platform === "linux" &&
	["flock", "sqlite3", "jq"].every((cmd) => spawnSync("which", [cmd]).status === 0);

/** Reap every owned child before deleting its files. */
afterEach(async () => {
	for (const { child, closed } of children.reverse()) {
		child.kill("SIGTERM");
		let timer: ReturnType<typeof setTimeout> | undefined;
		try {
			await Promise.race([
				closed,
				new Promise<never>((_, reject) => {
					timer = setTimeout(() => reject(new Error("child cleanup deadline")), 3000);
				}),
			]);
		} catch {
			child.kill("SIGKILL");
			await bounded(closed);
		} finally {
			clearTimeout(timer);
		}
	}
	children.length = 0;
	for (const root of owned) {
		if (process.env.ROLLOUT_ARTIFACT_DIR && existsSync(join(root, "deployments"))) {
			mkdirSync(process.env.ROLLOUT_ARTIFACT_DIR, { recursive: true });
			for (const name of readdirSync(join(root, "deployments"))) {
				const path = join(root, "deployments", name, "receipt.json");
				if (existsSync(path))
					writeFileSync(join(process.env.ROLLOUT_ARTIFACT_DIR, name + ".json"), readFileSync(path));
			}
		}
		rmSync(root, { recursive: true, force: true });
	}
	owned.length = 0;
});

/** Await confirmed completion with a deadline, including forced cleanup. */
async function bounded(closed: Promise<void>) {
	let timer: ReturnType<typeof setTimeout> | undefined;
	try {
		await Promise.race([
			closed,
			new Promise<never>((_, reject) => {
				timer = setTimeout(() => reject(new Error("unconfirmed child exit")), 3000);
			}),
		]);
	} finally {
		clearTimeout(timer);
	}
}

/** Bound startup by an explicit ready message, with close registered first. */
async function ready(args: string[]) {
	const child = spawn(process.execPath, args, { stdio: ["ignore", "pipe", "pipe"] });
	const closed = new Promise<void>((resolve) => child.once("close", () => resolve()));
	children.push({ child, closed });
	await new Promise<void>((resolve, reject) => {
		const timer = setTimeout(() => reject(new Error("child startup deadline")), 3000);
		const finish = (error?: Error) => {
			clearTimeout(timer);
			if (error) reject(error);
			else resolve();
		};
		child.once("error", finish);
		child.once("exit", () => finish(new Error("child exited before readiness")));
		child.stdout.on("data", (chunk: Buffer) => {
			if (chunk.toString().includes("ready")) finish();
		});
	});
	return child;
}
function fixture(name: string) {
	return readFileSync(join(fixtures, name));
}
function put(root: string, name: string, source: string) {
	copyFileSync(join(fixtures, source), join(root, name));
}
function sql(db: string, name: string) {
	const result = spawnSync("sqlite3", [db], { input: fixture(name), encoding: "utf8" });
	assert.equal(result.status, 0, result.stderr);
	return result.stdout;
}
function setup(fixed = false, plain = false) {
	const scratch = mkdtempSync(join(tmpdir(), "rollout-owned-"));
	owned.push(scratch);
	const root = fixed ? "/opt/monochange" : join(scratch, plain ? "deploy" : 'deploy space "quote');
	if (fixed) {
		assert.ok(
			existsSync("/.dockerenv") || existsSync("/run/.containerenv"),
			"fixed deployment paths may only be installed inside an isolated container",
		);
		assert.equal(existsSync(root), false);
		owned.push(root);
	}
	for (const dir of [
		root,
		join(root, "data"),
		join(root, "backups"),
		join(root, "secrets"),
		join(root, "caddy"),
		join(scratch, "bin"),
	])
		mkdirSync(dir, { recursive: true });
	put(root, "app.env", "app.env");
	put(join(root, "secrets"), "token", "token.txt");
	put(join(root, "caddy"), "Caddyfile", "Caddyfile");
	put(join(root, "backups"), "legacy.sqlite3", "token.txt");
	put(root, "health-body", "health-ok.json");
	put(root, "health-mode", "normal.txt");
	put(join(scratch, "bin"), "docker", "docker");
	chmodSync(join(scratch, "bin/docker"), 0o755);
	const db = join(root, "data/monochange_app.sqlite3");
	sql(db, "source.sql");
	const env = {
		...process.env,
		PATH: `${scratch}/bin:/usr/local/bin:/usr/bin:/bin`,
		MONOCHANGE_DEPLOY_DIR: root,
		MONOCHANGE_BACKUP_HELPER: join(scripts, "monochange-sqlite-backup.sh"),
		MONOCHANGE_HEALTH_TIMEOUT: "2",
		ROLLOUT_FIXTURES: fixtures,
	};
	return { root, db, env };
}
function run(
	env: NodeJS.ProcessEnv,
	args = [image, "1.2.3"],
	script = "monochange-deploy.sh",
	input?: Buffer,
) {
	return spawnSync(join(scripts, script), args, { env, input, encoding: "utf8", timeout: 10000 });
}
function receipts(root: string): Record<string, unknown>[] {
	return readdirSync(join(root, "deployments")).map((name) => {
		const receipt: unknown = JSON.parse(
			readFileSync(join(root, "deployments", name, "receipt.json"), "utf8"),
		);
		assert.ok(typeof receipt === "object" && receipt !== null && !Array.isArray(receipt));
		return Object.fromEntries(Object.entries(receipt));
	});
}
function preserved(root: string, db: string, rows = "42\n") {
	assert.equal(sql(db, "rows.sql"), rows);
	for (const [file, source] of [
		["app.env", "app.env"],
		["secrets/token", "token.txt"],
		["caddy/Caddyfile", "Caddyfile"],
		["backups/legacy.sqlite3", "token.txt"],
	])
		assert.deepEqual(readFileSync(join(root, file)), fixture(source));
}

describe.skipIf(!linux)("digitalocean transaction (real Linux SQLite, flock and HTTP)", () => {
	test.skipIf(
		process.getuid?.() !== 0 || (!existsSync("/.dockerenv") && !existsSync("/run/.containerenv")),
	)("daily prune failure preserves the newly validated snapshot", () => {
		const { root, db } = setup(false, true);
		const identity = spawnSync("id", ["-u", "nobody"], { encoding: "utf8" });
		assert.equal(identity.status, 0, identity.stderr);
		const uid = Number(identity.stdout.trim());
		assert.ok(Number.isInteger(uid) && uid > 0);
		for (const path of [dirname(root), root, join(root, "data"), db, join(root, "backups")]) {
			chownSync(path, uid, uid);
		}
		const daily = join(root, "backups/daily");
		mkdirSync(daily);
		chownSync(daily, uid, uid);
		const old = join(daily, "snapshot-AAAAAAAAAAAA");
		mkdirSync(old);
		copyFileSync(db, join(old, "monochange_app.sqlite3"));
		chmodSync(old, 0o555);
		const aged = new Date(Date.now() - 30 * 86400 * 1000);
		utimesSync(old, aged, aged);
		const result = spawnSync(
			"/usr/sbin/runuser",
			["-u", "nobody", "--", join(scripts, "monochange-sqlite-backup.sh"), "--daily"],
			{ env: { ...process.env, MONOCHANGE_DEPLOY_DIR: root }, encoding: "utf8", timeout: 10000 },
		);
		assert.equal(result.error, undefined);
		assert.notEqual(result.status, 0, "retention failure must be reported");
		const fresh = readdirSync(daily).filter((name) => name !== "snapshot-AAAAAAAAAAAA");
		assert.equal(fresh.length, 1);
		const retained = join(daily, fresh[0], "monochange_app.sqlite3");
		assert.ok(existsSync(retained), "a prune failure must not discard a validated snapshot");
		assert.equal(sql(retained, "rows.sql"), "42\n");
		assert.equal(existsSync(join(daily, fresh[0], "prune-list")), false);
	});
	test("WAL snapshot, newer writes, settings and unique same-SHA receipts survive success and failure", async () => {
		const { root, db, env } = setup();
		await ready([join(fixtures, "health-server.ts"), root]);
		await ready([join(fixtures, "wal-writer.ts"), db, fixtures]);
		assert.ok(existsSync(`${db}-wal`) && readFileSync(`${db}-wal`).length > 0);
		assert.equal(run(env).status, 0);
		const first = receipts(root)[0];
		assert.equal(typeof first.backupPath, "string");
		assert.equal(sql(String(first.backupPath), "rows.sql"), "42\n99\n");
		sql(db, "newer.sql");
		assert.equal(run(env).status, 0);
		put(root, "compose-fail", "token.txt");
		assert.notEqual(run(env).status, 0);
		const all = receipts(root);
		assert.equal(all.length, 3);
		assert.equal(all.filter((r) => r.outcome === "success").length, 2);
		assert.ok(all.every((r) => r.backupDigest));
		assert.equal(readFileSync(join(root, ".website-version"), "utf8"), "1.2.3\n");
		preserved(root, db, "42\n99\n101\n");
	});
	test("image, stable version and incompatible options fail before mutation", () => {
		const { root, db, env } = setup();
		for (const args of [
			["bad", "1.2.3"],
			[image, "01.2.3"],
			[image, "1.2.3-beta"],
			["--rollback", "--load-stdin", image, "1.2.3"],
			["--bootstrap", "--rollback", image, "1.2.3"],
		])
			assert.notEqual(run(env, args).status, 0);
		put(root, "image-missing", "token.txt");
		assert.notEqual(run(env).status, 0);
		assert.equal(existsSync(join(root, ".env")), false);
		preserved(root, db);
	});
	test("downgrade denied; explicit image rollback preserves newer data", async () => {
		const { root, db, env } = setup();
		await ready([join(fixtures, "health-server.ts"), root]);
		put(root, ".website-version", "version.txt");
		sql(db, "newer.sql");
		assert.match(run(env).stderr, /newer website/);
		assert.equal(run(env, ["--rollback", image, "1.2.3"]).status, 0);
		preserved(root, db, "42\n101\n");
	});
	test.each(["legacy-env.txt", "malformed-env.txt"])(
		"legacy dotenv %s refuses without disclosing content",
		(source) => {
			const { root, env } = setup();
			put(root, ".env", source);
			const result = run(env);
			assert.notEqual(result.status, 0);
			assert.match(result.stderr, /migrate settings/);
			assert.ok(
				!result.stderr.includes("supersecret") && !result.stderr.includes("malformed-secret"),
			);
			assert.deepEqual(readFileSync(join(root, ".env")), fixture(source));
		},
	);
	test.each(["error", "hang", "slow", "wrong", "malformed"])(
		"health %s fails within elapsed deadline",
		async (mode) => {
			const { root, db, env } = setup();
			if (["error", "hang", "slow"].includes(mode)) put(root, "health-mode", `${mode}.txt`);
			if (mode === "wrong") put(root, "health-body", "health-stale.json");
			if (mode === "malformed") put(root, "health-body", "malformed.json");
			await ready([join(fixtures, "health-server.ts"), root]);
			put(root, ".env", "desired.env");
			const start = Date.now();
			const result = run(env);
			assert.notEqual(result.status, 0);
			assert.ok(Date.now() - start < 3500, result.stderr);
			assert.equal(existsSync(join(root, ".website-version")), false);
			assert.equal(receipts(root)[0].lastVerifiedBefore, null);
			preserved(root, db);
		},
	);
	test.each(["fail-backup", "corrupt-backup", "fk-error", "corrupt-db"])(
		"backup %s aborts before compose and desired mutation",
		(mode) => {
			const { root, db, env } = setup();
			if (mode.endsWith("backup")) env.MONOCHANGE_BACKUP_HELPER = join(fixtures, mode);
			if (mode === "fk-error") {
				rmSync(db);
				sql(db, "fk-error.sql");
			}
			if (mode === "corrupt-db") put(join(root, "data"), "monochange_app.sqlite3", "corrupt.txt");
			const result = run(env);
			assert.notEqual(result.status, 0);
			assert.equal(existsSync(join(root, ".env")), false);
			assert.equal(existsSync(join(root, "compose-entered")), false);
		},
	);
	test("foreign-key command error with empty stdout fails standalone snapshot", () => {
		const { root, db, env } = setup(false, true);
		rmSync(db);
		sql(db, "fk-error.sql");
		const command = spawnSync("sqlite3", [db], {
			input: fixture("fk-check.sql"),
			encoding: "utf8",
		});
		assert.notEqual(command.status, 0);
		assert.equal(command.stdout, "");
		const result = run(env, [], "monochange-sqlite-backup.sh");
		assert.notEqual(result.status, 0);
		assert.ok(!result.stderr.includes("parent(value"));
		assert.equal(readdirSync(join(root, "backups")).length, 1);
	});
	test("interruption retains recovery evidence and never advances markers", async () => {
		const { root, db, env } = setup();
		put(root, "hold-compose", "token.txt");
		const child = spawn(join(scripts, "monochange-deploy.sh"), [image, "1.2.3"], { env });
		const closed = new Promise<void>((resolve) => child.once("close", () => resolve()));
		children.push({ child, closed });
		try {
			const deadline = Date.now() + 3000;
			while (!existsSync(join(root, "compose-entered")) && Date.now() < deadline)
				await new Promise((resolve) => setTimeout(resolve, 20));
			assert.ok(existsSync(join(root, "compose-entered")));
			child.kill("SIGTERM");
		} finally {
			rmSync(join(root, "hold-compose"), { force: true });
			await bounded(closed);
		}
		assert.equal(child.exitCode, 143);
		assert.equal(receipts(root)[0].outcome, "interrupted");
		assert.equal(existsSync(join(root, ".website-version")), false);
		preserved(root, db);
	});
	test("invalid prior metadata refuses before mutation", () => {
		const { root, env } = setup();
		put(root, ".website-verified.json", "malformed.json");
		assert.notEqual(run(env).status, 0);
		assert.equal(existsSync(join(root, ".env")), false);
	});
	test("missing existing DB fails; bootstrap refuses history; explicit first bootstrap succeeds", async () => {
		const { root, db, env } = setup();
		rmSync(db);
		assert.notEqual(run(env).status, 0);
		assert.notEqual(run(env, ["--bootstrap", image, "1.2.3"]).status, 0);
		rmSync(join(root, "backups"), { recursive: true });
		await ready([join(fixtures, "health-server.ts"), root]);
		assert.equal(run(env, ["--bootstrap", image, "1.2.3"]).status, 0);
		assert.notEqual(run(env, ["--bootstrap", image, "1.2.3"]).status, 0);
	});
	test("daily retention preserves legacy, release and unrelated daily files", () => {
		const { root, db, env } = setup();
		assert.equal(run(env, ["--daily"], "monochange-sqlite-backup.sh").status, 0);
		const daily = join(root, "backups/daily");
		const old = join(daily, readdirSync(daily)[0]);
		const past = new Date(Date.now() - 30 * 86400000);
		utimesSync(old, past, past);
		put(daily, "unrelated", "token.txt");
		assert.equal(run(env, ["--daily"], "monochange-sqlite-backup.sh").status, 0);
		assert.equal(existsSync(old), false);
		assert.ok(existsSync(join(daily, "unrelated")));
		preserved(root, db);
	});
	test.skipIf(process.env.ROLLOUT_FIXED_PATHS !== "1")(
		"unmodified installed CI valid wire loads gzip, backs up once, checks exact health, shares manual lock",
		async () => {
			// Only enabled in a dedicated throwaway container, never on a server.
			const { root, db, env } = setup(true);
			for (const [src, dest] of [
				["monochange-deploy.sh", "monochange-deploy"],
				["monochange-sqlite-backup.sh", "monochange-sqlite-backup"],
				["ci-deploy.sh", "monochange-ci-deploy"],
			]) {
				copyFileSync(join(scripts, src), `/usr/local/bin/${dest}`);
				chmodSync(`/usr/local/bin/${dest}`, 0o755);
			}
			assert.equal(existsSync("/usr/local/bin/docker"), false);
			copyFileSync(join(fixtures, "docker"), "/usr/local/bin/docker");
			chmodSync("/usr/local/bin/docker", 0o755);
			try {
				await ready([join(fixtures, "health-server.ts"), root]);
				const input = spawnSync("gzip", ["-c"], { input: fixture("load.txt") }).stdout;
				const ciEnv = {
					...env,
					SSH_ORIGINAL_COMMAND: `deploy ${"a".repeat(40)} 1.2.3`,
					MONOCHANGE_DEPLOY_DIR: "/untrusted",
					MONOCHANGE_BACKUP_HELPER: "/untrusted",
					MONOCHANGE_HEALTH_TIMEOUT: "0",
				};
				const result = run(ciEnv, [], "ci-deploy.sh", input);
				assert.equal(result.status, 0, result.stderr);
				assert.deepEqual(readFileSync(join(root, "loaded.txt")), fixture("load.txt"));
				assert.equal(
					readdirSync(join(root, "backups")).filter((p) => p.startsWith("snapshot-")).length,
					1,
				);
				assert.equal(receipts(root)[0].outcome, "success");
				preserved(root, db);
				for (const wire of [
					"deploy bad 1.2.3",
					`deploy ${"a".repeat(40)} 1.2.3 --rollback`,
					`deploy ${"a".repeat(40)} 1.2.3; true`,
				])
					assert.notEqual(
						run({ ...ciEnv, SSH_ORIGINAL_COMMAND: wire }, [], "ci-deploy.sh").status,
						0,
					);
				// Manual transaction blocks inside the real Compose call after acquiring flock.
				put(root, "hold-compose", "token.txt");
				rmSync(join(root, "compose-entered"));
				const manual = spawn(join(scripts, "monochange-deploy.sh"), [image, "1.2.3"], { env });
				const closed = new Promise<void>((resolve) => manual.once("close", () => resolve()));
				children.push({ child: manual, closed });
				const deadline = Date.now() + 3000;
				while (!existsSync(join(root, "compose-entered")) && Date.now() < deadline)
					await new Promise((resolve) => setTimeout(resolve, 20));
				assert.ok(existsSync(join(root, "compose-entered")));
				let contended;
				try {
					contended = run(ciEnv, [], "ci-deploy.sh", input);
				} finally {
					rmSync(join(root, "hold-compose"), { force: true });
					await bounded(closed);
				}
				assert.match(contended.stderr, /another website deployment/);
				assert.equal(manual.exitCode, 0);
			} finally {
				rmSync("/usr/local/bin/docker", { force: true });
			}
		},
	);
});
