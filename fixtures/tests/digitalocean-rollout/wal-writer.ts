import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
const writer = spawn("sqlite3", [process.argv[2]], { stdio: ["pipe", "pipe", "inherit"] });
writer.stdout.on("data", (chunk: Buffer) => process.stdout.write(chunk));
writer.stdin.write(readFileSync(`${process.argv[3]}/wal.sql`));
process.on("SIGTERM", () => writer.stdin.end());
writer.on("close", (code) => process.exit(code ?? 1));
