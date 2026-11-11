#!/usr/bin/env node
// Tries to bind the port it is given and exits nonzero when it cannot: the
// stale-responder fixture. A stale process already holding the port keeps
// answering /health, so a harness that only polls for health would wrongly
// accept it; this fixture mirrors a real backend exiting on a bind failure so
// the harness must notice the child it owns has ended.
import { writeFileSync } from "node:fs";
import net from "node:net";

const port = Number(process.env.PORT ?? "0");
const pidfile = process.env.VERIFY_PIDFILE;

if (pidfile !== undefined && pidfile !== "") {
	writeFileSync(pidfile, `${process.pid}\n`);
}

const socket = net.createServer();

socket.once("error", (error) => {
	console.error(`fail-on-port fixture: could not bind ${port}: ${error}`);
	process.exit(1);
});

socket.listen(port, "127.0.0.1", () => {
	socket.close(() => {
		process.exit(0);
	});
});
