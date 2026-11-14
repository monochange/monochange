// Keep running after stdin closes so cleanup must terminate and reap us.
process.stdin.resume();
setInterval(() => {}, 30_000);
process.stdout.write("ready\n");
