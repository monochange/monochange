#!/usr/bin/env node
// Exits during startup with a nonzero code, after writing its pid to
// VERIFY_PIDFILE: the partial-startup-failure fixture. The harness must
// report the early exit (not hang, not accept any other responder) and still
// reap what it spawned.
import { writeFileSync } from "node:fs";

const pidfile = process.env.VERIFY_PIDFILE;

if (pidfile !== undefined && pidfile !== "") {
	writeFileSync(pidfile, `${process.pid}\n`);
}

console.error("fail-fast fixture: exiting during startup");
process.exit(3);
