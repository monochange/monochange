#!/usr/bin/env node
/**
 * Local rehearsal of the SQLite backup mechanism that the project deploys.
 *
 * Production (app/deploy/digitalocean/harden-droplet.sh) installs
 * `/usr/local/bin/monochange-sqlite-backup`, whose entire mechanism is one
 * online snapshot — `sqlite3 "$DB" ".backup $BACKUP_DIR/monochange_app-<UTC
 * timestamp>.sqlite3"` — plus a 14-day `find -mtime +14 -delete` retention
 * sweep. Restoring is a deliberate operator copy over the live database after
 * stopping the app (app/DEPLOY.md, "backups").
 *
 * This tool rehearses exactly that mechanism against a deliberately fake
 * fixture database built from the real release-upgrade schema
 * (fixtures/tests/app-sqlite-release-upgrade/), and pins the restore contract
 * the docs describe:
 *
 * - a snapshot taken while committed data still sits in an uncheckpointed WAL
 *   file contains that data, while a torn copy of the main database file
 *   alone misses it — the reason the `.backup` mechanism exists;
 * - restoring verifies the snapshot (quick_check, integrity_check, and
 *   foreign_key_check) BEFORE replacing the destination, so a corrupt or
 *   missing backup can never clobber the last-good database;
 * - the source database is untouched by a restore, and the rehearsal's
 *   post-upgrade writes survive it — no automatic rollback overwrites newer
 *   data, because there is no automatic rollback.
 *
 * Retention is intentionally not re-implemented here; it stays with the
 * deployed cron helper and its documented 14-day window.
 */

import { spawn, spawnSync } from "node:child_process";
import {
	copyFileSync,
	existsSync,
	mkdirSync,
	readFileSync,
	statSync,
	writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createHash } from "node:crypto";

const __dirname = dirname(fileURLToPath(import.meta.url));
export const fixtureDir = resolve(__dirname, "../fixtures/tests/app-sqlite-release-upgrade");

/** A named rehearsal step with the evidence that proves it passed. */
export interface RehearsalStep {
	name: string;
	detail: string;
}

export interface RehearsalResult {
	steps: RehearsalStep[];
	sourceDb: string;
	backupPath: string;
	restorePath: string;
}

export interface IntegrityReport {
	quickCheck: string;
	integrityCheck: string;
	foreignKeyViolations: string[];
}

/**
 * Resolve the sqlite3 CLI the deployed helper runs. The caller's override
 * wins, then PATH, then the standard host locations (the nix-only devenv
 * PATH intentionally omits /usr/bin, where macOS and Ubuntu both ship it).
 */
export function resolveSqlite3(sqlite3Bin?: string): string {
	// An explicit override is a configuration choice, not a fallback: if the
	// caller or environment names a binary, a failure to run it is an error
	// rather than a silent fallthrough to another location.
	const explicit = sqlite3Bin ?? process.env.SQLITE3_BIN;
	if (explicit !== undefined && explicit.length > 0) {
		const probe = spawnSync(explicit, ["--version"], { encoding: "utf8" });
		if (probe.error === undefined && probe.status === 0) {
			return explicit;
		}
		const reason =
			probe.error !== undefined
				? probe.error.message
				: `sqlite3 exited ${probe.status}: ${(probe.stderr ?? "").trim()}`;
		throw new Error(`explicit sqlite3 override is invalid or unavailable: ${explicit} (${reason})`);
	}

	const candidates = [
		"sqlite3",
		"/usr/bin/sqlite3",
		"/opt/homebrew/bin/sqlite3",
		"/usr/local/bin/sqlite3",
	];
	for (const bin of candidates) {
		const probe = spawnSync(bin, ["--version"], { encoding: "utf8" });
		if (probe.error === undefined && probe.status === 0) {
			return bin;
		}
	}
	throw new Error(
		`the sqlite3 CLI is required to rehearse the deployed backup mechanism (tried: ${candidates.join(", ")}); install it or point SQLITE3_BIN at it`,
	);
}

function requireSqlite3(sqlite3Bin?: string): string {
	return resolveSqlite3(sqlite3Bin);
}

function runSqlite(bin: string, dbPath: string, sql: string): string {
	const run = spawnSync(bin, [dbPath], { input: sql, encoding: "utf8" });
	if (run.error !== undefined) {
		throw new Error(`sqlite3 failed for ${dbPath}: ${run.error.message}`);
	}
	if (run.status !== 0) {
		throw new Error(`sqlite3 exited ${run.status} for ${dbPath}: ${run.stderr.trim()}`);
	}
	return run.stdout;
}

function readFixture(name: string): string {
	return readFileSync(join(fixtureDir, name), "utf8");
}

/** Run the three integrity pragmas the rehearsal contract relies on. */
export function verifyIntegrity(bin: string, dbPath: string): IntegrityReport {
	const pragmas = "PRAGMA quick_check;\nPRAGMA integrity_check;\nPRAGMA foreign_key_check;\n";
	const run = spawnSync(bin, [dbPath], { input: pragmas, encoding: "utf8" });
	if (run.error !== undefined) {
		throw new Error(`sqlite3 failed for ${dbPath}: ${run.error.message}`);
	}
	// A database corrupt enough to make the pragmas themselves fail is still
	// an integrity verification failure, with the same contract wording.
	if (run.status !== 0) {
		throw new Error(
			`integrity verification failed for ${dbPath}: sqlite3 exited ${run.status}: ${run.stderr.trim()}`,
		);
	}
	const lines = run.stdout.split("\n").filter((line) => line.length > 0);
	const quickCheck = lines[0] ?? "";
	const integrityCheck = lines[1] ?? "";
	const foreignKeyViolations = lines.slice(2);
	if (quickCheck !== "ok" || integrityCheck !== "ok" || foreignKeyViolations.length > 0) {
		throw new Error(
			`integrity verification failed for ${dbPath}: quick_check=${quickCheck}, integrity_check=${integrityCheck}, foreign_key_check=${foreignKeyViolations.length} rows`,
		);
	}
	return { quickCheck, integrityCheck, foreignKeyViolations };
}

/** List every non-system table with its live row count, sorted by name. */
export function tableRowCounts(bin: string, dbPath: string): Array<[string, number]> {
	const tables = runSqlite(
		bin,
		dbPath,
		"SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name;\n",
	)
		.split("\n")
		.filter((line) => line.length > 0);
	return tables.map((table) => [
		table,
		Number.parseInt(runSqlite(bin, dbPath, `SELECT count(*) FROM "${table}";\n`).trim(), 10),
	]);
}

function sha256File(path: string): string {
	return createHash("sha256").update(readFileSync(path)).digest("hex");
}

/**
 * The deployed mechanism, verbatim: `sqlite3 "$DB" ".backup $DEST"` from
 * harden-droplet.sh's monochange-sqlite-backup.
 */
export function backupDatabase(bin: string, dbPath: string, backupPath: string): void {
	runSqlite(bin, dbPath, `.backup '${backupPath}'\n`);
}

/**
 * Restore contract: verify the snapshot before touching the destination.
 *
 * Production restores by copying a snapshot over the stopped app's database;
 * this rehearsal keeps the destination a DIFFERENT path and fails closed on a
 * corrupt or missing snapshot so a last-good destination is never replaced by
 * an unusable file.
 */
export function restoreBackup(bin: string, backupPath: string, destinationPath: string): void {
	if (!existsSync(backupPath)) {
		throw new Error(`refusing to restore: backup ${backupPath} does not exist`);
	}
	verifyIntegrity(bin, backupPath);
	copyFileSync(backupPath, destinationPath);
	verifyIntegrity(bin, destinationPath);
}

/**
 * Hold one open sqlite3 writer connection that has committed rows into the
 * WAL without checkpointing — the state a live monochange_app process leaves
 * the production database in while the cron backup runs.
 */
async function withOpenWalWriter(
	bin: string,
	dbPath: string,
	whileOpen: () => void,
): Promise<void> {
	const writer = spawn(bin, [dbPath], { stdio: ["pipe", "ignore", "pipe"] });
	let stderr = "";
	let exited = false;
	// Register completion before any input is written or ended, so a fast
	// exit is never missed and the await below cannot hang.
	writer.on("exit", (code) => {
		exited = true;
		if (code !== null && code !== 0) {
			stderr += ` writer exited ${code}`;
		}
	});
	writer.stderr?.on("data", (chunk: Buffer) => {
		stderr += chunk.toString();
	});
	writer.on("error", (error) => {
		stderr += error.message;
	});

	const walPath = `${dbPath}-wal`;
	let writerInputError: Error | null = null;
	try {
		writer.stdin?.write(readFixture("uncheckpointed-wal-row.sql"));
	} catch (error) {
		writerInputError = error instanceof Error ? error : new Error(String(error));
	}
	if (writerInputError !== null) {
		writer.stdin?.end();
		await reapWriter(writer, 5_000);
		throw writerInputError;
	}

	// Wait until the WAL file holds bytes AND a separate reader can see the
	// committed row. Nonzero WAL size alone can precede the commit marker.
	const startupDeadline = Date.now() + 5_000;
	while (true) {
		if (exited) {
			throw new Error(`the open writer exited before committing WAL frames: ${stderr.trim()}`);
		}
		const walReady =
			existsSync(walPath) && statSync(walPath).size > 0 && committedWalRow(bin, dbPath);
		if (walReady) {
			break;
		}
		if (Date.now() > startupDeadline) {
			writer.stdin?.end();
			// eslint-disable-next-line eslint/no-await-in-loop -- cleanup of the timed-out writer must finish before throwing.
			await reapWriter(writer, 5_000);
			throw new Error(
				`the open writer never produced a committed WAL transaction: ${stderr.trim()}`,
			);
		}
		// eslint-disable-next-line eslint/no-await-in-loop -- WAL polling is sequential by contract: each bounded wait only starts after the previous check found no frames yet.
		await new Promise<void>((resolveWait) => {
			setTimeout(resolveWait, 10);
		});
	}

	try {
		whileOpen();
	} finally {
		writer.stdin?.end();
		await reapWriter(writer, 5_000);
	}

	if (stderr.trim().length > 0) {
		throw new Error(`open WAL writer reported errors: ${stderr.trim()}`);
	}
}

function committedWalRow(bin: string, dbPath: string): boolean {
	try {
		return (
			runSqlite(
				bin,
				dbPath,
				"SELECT count(*) FROM feedback_notifications WHERE id = 3;\n",
			).trim() === "1"
		);
	} catch {
		return false;
	}
}

async function reapWriter(writer: ReturnType<typeof spawn>, timeoutMs: number): Promise<void> {
	if (exitedFlag(writer)) {
		return;
	}
	await new Promise<void>((resolveReap) => {
		const timer = setTimeout(() => {
			try {
				writer.kill("SIGKILL");
			} catch {
				// The process may already be gone; ignore.
			}
			resolveReap();
		}, timeoutMs);
		writer.once("exit", () => {
			clearTimeout(timer);
			resolveReap();
		});
	});
}

function exitedFlag(writer: ReturnType<typeof spawn>): boolean {
	return writer.exitCode !== null || writer.signalCode !== null;
}

/**
 * Run the whole rehearsal inside `dataDir` and return the per-step evidence.
 * Every record the rehearsal writes is deliberately fake fixture data.
 */
export async function rehearse(options: {
	dataDir: string;
	sqlite3Bin?: string;
}): Promise<RehearsalResult> {
	const bin = requireSqlite3(options.sqlite3Bin);
	const steps: RehearsalStep[] = [];
	mkdirSync(options.dataDir, { recursive: true });
	const sourceDb = join(options.dataDir, "data", "monochange_app.sqlite3");
	const backupDir = join(options.dataDir, "backups");
	const backupPath = join(backupDir, "monochange_app-rehearsal.sqlite3");
	const restorePath = join(options.dataDir, "restored-data", "monochange_app.sqlite3");
	const tornCopyPath = join(options.dataDir, "torn-copy.sqlite3");
	mkdirSync(dirname(sourceDb), { recursive: true });
	mkdirSync(backupDir, { recursive: true });
	mkdirSync(dirname(restorePath), { recursive: true });

	// 1. Build the v0.3.3-shaped database and seed it with fake records, then
	// apply the candidate migrations — the same schema text the Rust
	// release-upgrade tests prove byte-equal to the real migration writers.
	runSqlite(bin, sourceDb, readFixture("legacy-schema.sql"));
	runSqlite(bin, sourceDb, readFixture("seed.sql"));
	runSqlite(bin, sourceDb, readFixture("candidate-schema.sql"));
	const preWriteCounts = tableRowCounts(bin, sourceDb);
	steps.push({
		name: "build-upgraded-fixture-database",
		detail: `${preWriteCounts.length} tables, ${preWriteCounts.reduce((sum, [, count]) => sum + count, 0)} rows`,
	});

	// 2. First writes after the upgrade: a new user (session_epoch defaults to
	// 0) — the candidate release's own schema in use.
	runSqlite(bin, sourceDb, readFixture("post-upgrade-user.sql"));
	const epoch = runSqlite(bin, sourceDb, "SELECT session_epoch FROM users WHERE id = 4;\n").trim();
	if (epoch !== "0") {
		throw new Error(`post-upgrade user session_epoch should default to 0, got ${epoch}`);
	}
	steps.push({ name: "post-upgrade-write", detail: `users row 4 with session_epoch ${epoch}` });

	// 3. Take the deployed snapshot while a writer holds a committed row in an
	// uncheckpointed WAL, and prove a torn main-file copy misses that row.
	let walBytes = 0;
	await withOpenWalWriter(bin, sourceDb, () => {
		copyFileSync(sourceDb, tornCopyPath);
		const tornHasRow = runSqlite(
			bin,
			tornCopyPath,
			"SELECT count(*) FROM feedback_notifications WHERE id = 3;\n",
		).trim();
		if (tornHasRow !== "0") {
			throw new Error("torn copy unexpectedly contains the WAL-only row");
		}
		backupDatabase(bin, sourceDb, backupPath);
		walBytes = statSync(`${sourceDb}-wal`).size;
	});
	steps.push({
		name: "online-backup-with-uncheckpointed-wal",
		detail: `${walBytes} WAL bytes outstanding; torn main-file copy missed the WAL-only row`,
	});

	// 4. The snapshot must contain every epoch of data: seed, post-upgrade
	// write, and the committed WAL-only row.
	verifyIntegrity(bin, backupPath);
	const backupHasWalRow = runSqlite(
		bin,
		backupPath,
		"SELECT count(*) FROM feedback_notifications WHERE id = 3;\n",
	).trim();
	if (backupHasWalRow !== "1") {
		throw new Error("backup is missing the committed WAL-only row");
	}
	steps.push({
		name: "snapshot-is-consistent",
		detail: "quick_check/integrity_check ok and the WAL-only row is present",
	});

	// 5. Restore into a DIFFERENT destination, verify, and prove the source is
	// untouched. The post-upgrade writes survive: a restore never happens on
	// its own, so newer data is only ever lost by a deliberate operator copy.
	const sourceHashBefore = sha256File(sourceDb);
	restoreBackup(bin, backupPath, restorePath);
	if (sourceHashBefore !== sha256File(sourceDb)) {
		throw new Error("restore modified the source database");
	}
	// A restored snapshot is a consistent copy, not a byte-identical twin of
	// the live files, so the last-good destination hash is its own.
	const restoredHash = sha256File(restorePath);
	const restoredCounts = tableRowCounts(bin, restorePath);
	const restoredEpochsZero = runSqlite(
		bin,
		restorePath,
		"SELECT count(*) FROM users WHERE session_epoch = 0;\n",
	).trim();
	if (restoredEpochsZero !== "4") {
		throw new Error(`restored users should all read session_epoch 0, got ${restoredEpochsZero}/4`);
	}
	steps.push({
		name: "restore-into-different-destination",
		detail: `${restoredCounts.reduce((sum, [, count]) => sum + count, 0)} rows across ${restoredCounts.length} tables; source unchanged; post-upgrade writes intact`,
	});

	// 6. A corrupt snapshot must fail verification before replacing the
	// last-good destination.
	const corruptPath = join(backupDir, "monochange_app-corrupt.sqlite3");
	const corrupted = Buffer.from(readFileSync(backupPath));
	// Overwrite interior pages with garbage so no pragma can read them.
	corrupted.subarray(3 * 4096, 6 * 4096).fill(0xde);
	corrupted.subarray(9 * 4096, 12 * 4096).fill(0xad);
	writeFileSync(corruptPath, corrupted);
	let corruptFailed = false;
	try {
		restoreBackup(bin, corruptPath, restorePath);
	} catch {
		corruptFailed = true;
	}
	if (!corruptFailed) {
		throw new Error("corrupt snapshot was restored without failing verification");
	}
	if (sha256File(restorePath) !== restoredHash) {
		throw new Error("a failed restore replaced the last-good destination");
	}
	steps.push({
		name: "corrupt-snapshot-fails-before-replacing",
		detail: "restore refused the corrupt snapshot; destination byte-identical",
	});

	// 7. A missing snapshot fails the same way.
	let missingFailed = false;
	try {
		restoreBackup(bin, join(backupDir, "does-not-exist.sqlite3"), restorePath);
	} catch {
		missingFailed = true;
	}
	if (!missingFailed) {
		throw new Error("missing snapshot was restored without failing");
	}
	if (sha256File(restorePath) !== restoredHash) {
		throw new Error("a failed missing-snapshot restore replaced the last-good destination");
	}
	steps.push({
		name: "missing-snapshot-fails-before-replacing",
		detail: "restore refused the missing snapshot; destination byte-identical",
	});

	return { steps, sourceDb, backupPath, restorePath };
}

const isMain =
	process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMain) {
	const dataDir = process.argv[2];
	if (dataDir === undefined || dataDir.length === 0) {
		console.error("usage: node scripts/sqlite-backup-rehearsal.ts <data-dir>");
		process.exit(2);
	}
	try {
		const result = await rehearse({ dataDir });
		for (const step of result.steps) {
			console.log(`ok ${step.name}: ${step.detail}`);
		}
		console.log(`source   ${result.sourceDb}`);
		console.log(`backup   ${result.backupPath}`);
		console.log(`restored ${result.restorePath}`);
	} catch (error) {
		console.error(`rehearsal failed: ${error instanceof Error ? error.message : String(error)}`);
		process.exit(1);
	}
}
