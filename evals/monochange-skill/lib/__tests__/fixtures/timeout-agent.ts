#!/usr/bin/env node
import { spawn } from "node:child_process";
import process from "node:process";

if (process.argv.includes("--child")) {
	setTimeout(() => process.stdout.write("Child retained the pipe\n"), 3000);
} else {
	spawn(process.execPath, [import.meta.filename, "--child"], { stdio: "inherit" });
	setInterval(() => {}, 1000);
}
