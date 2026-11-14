#!/usr/bin/env node
// Stays alive without ever becoming healthy: the startup-timeout fixture.
// The harness must hit its deadline, then terminate this process it owns.
import { writeFileSync } from "node:fs";

const pidfile = process.env.VERIFY_PIDFILE;

if (pidfile !== undefined && pidfile !== "") {
	writeFileSync(pidfile, `${process.pid}\n`);
}

console.log("never-healthy fixture: staying alive without a health endpoint");
setInterval(() => {}, 1_000_000);
