import { createServer } from "node:http";
import { readFileSync } from "node:fs";
const root = process.argv[2];
const server = createServer((_req, res) => {
 const mode = readFileSync(`${root}/health-mode`, "utf8").trim();
 if (mode === "hang") return;
 const send = () => {
  res.writeHead(mode === "error" ? 503 : 200);
  res.end(readFileSync(`${root}/health-body`, "utf8"));
 };
 if (mode === "slow") setTimeout(send, 5000); else send();
});
server.listen(3000, "127.0.0.1", () => process.stdout.write("ready\n"));
process.on("SIGTERM", () => { server.closeAllConnections(); server.close(() => process.exit()); });
