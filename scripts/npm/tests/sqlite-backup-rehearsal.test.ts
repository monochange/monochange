import { spawnSync } from "node:child_process";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, test } from "vitest";
import {
	backupDatabase,
	rehearse,
	resolveSqlite3,
	restoreBackup,
	tableRowCounts,
	verifyIntegrity,
} from "../../sqlite-backup-rehearsal.ts";

/** Run SQL through the same deployed CLI the rehearsal drives. */
function runSqlite(bin: string, dbPath: string, sql: string): void {
	const run = spawnSync(bin, [dbPath], { input: sql, encoding: "utf8" });
	if (run.error !== undefined) {
		throw new Error(`sqlite3 is required for the backup rehearsal tests: ${run.error.message}`);
	}
	if (run.status !== 0) {
		throw new Error(`sqlite3 exited ${run.status}: ${run.stderr}`);
	}
}

describe("sqlite backup rehearsal", () => {
	const bin = resolveSqlite3();
	const runDirs: string[] = [];

	afterAll(() => {
		for (const dir of runDirs) {
			rmSync(dir, { recursive: true, force: true });
		}
	});

	function makeRunDir(): string {
		const dir = mkdtempSync(join(tmpdir(), "sqlite-backup-rehearsal-"));
		runDirs.push(dir);
		return dir;
	}

	test("the deployed snapshot mechanism keeps uncheckpointed WAL data and restores independently", async () => {
		const result = await rehearse({ dataDir: makeRunDir() });
		assert.deepEqual(
			result.steps.map((step) => step.name),
			[
				"build-upgraded-fixture-database",
				"post-upgrade-write",
				"online-backup-with-uncheckpointed-wal",
				"snapshot-is-consistent",
				"restore-into-different-destination",
				"corrupt-snapshot-fails-before-replacing",
				"missing-snapshot-fails-before-replacing",
			],
		);
		// The backup and the restore are DIFFERENT files in different
		// directories: the rehearsal never restores over its own source.
		assert.notEqual(result.backupPath, result.sourceDb);
		assert.notEqual(result.restorePath, result.sourceDb);
		assert.notEqual(result.restorePath, result.backupPath);
	}, 30_000);

	test("verifyIntegrity rejects a corrupt database before any restore decision", () => {
		const dir = makeRunDir();
		const good = join(dir, "good.sqlite3");
		runSqlite(bin, good, "CREATE TABLE t (v INTEGER);\nINSERT INTO t VALUES (1);\n");
		assert.equal(verifyIntegrity(bin, good).quickCheck, "ok");

		const corrupt = join(dir, "corrupt.sqlite3");
		runSqlite(bin, corrupt, "CREATE TABLE t (v BLOB);\n");
		runSqlite(
			bin,
			corrupt,
			"INSERT INTO t VALUES (randomblob(4096));\nINSERT INTO t VALUES (randomblob(4096));\n",
		);
		const bytes = Buffer.from(readFileSync(corrupt));
		bytes.fill(0x00, 4096, 8192);
		writeFileSync(corrupt, bytes);
		assert.throws(() => verifyIntegrity(bin, corrupt), /integrity verification failed/);
	});

	test("restoreBackup fails closed on a missing snapshot without touching the destination", () => {
		const dir = makeRunDir();
		const destination = join(dir, "last-good.sqlite3");
		runSqlite(bin, destination, "CREATE TABLE t (v INTEGER);\n");
		assert.throws(
			() => restoreBackup(bin, join(dir, "absent.sqlite3"), destination),
			/refusing to restore/,
		);
		assert.equal(tableRowCounts(bin, destination).length, 1);
	});

	test("backupDatabase produces a file sqlite3 can verify independently", () => {
		const dir = makeRunDir();
		const source = join(dir, "source.sqlite3");
		runSqlite(bin, source, "CREATE TABLE t (v INTEGER);\nINSERT INTO t VALUES (7);\n");
		const snapshot = join(dir, "snapshot.sqlite3");
		backupDatabase(bin, source, snapshot);
		assert.equal(verifyIntegrity(bin, snapshot).quickCheck, "ok");
		assert.equal(tableRowCounts(bin, snapshot)[0]?.[1], 1);
	});
});
