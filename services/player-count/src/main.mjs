/**
 * Metallic Studios — player-count aggregator entry point.
 *
 * Starts the local/self-hosted aggregator HTTP service. Real deployment needs a
 * protected HTTPS edge in front of it and an authenticated room-server fleet
 * pushing heartbeats; neither exists in this repository yet.
 */
import { PlayerCountAggregator } from "./aggregator.mjs";
import { createPlayerCountServer } from "./server.mjs";

const ingestToken = process.env.METALLIC_PLAYER_COUNT_INGEST_TOKEN;
const readToken = process.env.METALLIC_PLAYER_COUNT_READ_TOKEN;
const environment = process.env.METALLIC_PLAYER_COUNT_ENV || "production";
const host = process.env.METALLIC_PLAYER_COUNT_HOST || "127.0.0.1";
const port = Number(process.env.METALLIC_PLAYER_COUNT_PORT || 4040);

if (!ingestToken || !readToken) {
  throw new Error(
    "METALLIC_PLAYER_COUNT_INGEST_TOKEN and METALLIC_PLAYER_COUNT_READ_TOKEN are required",
  );
}
if (!Number.isInteger(port) || port < 1 || port > 65535) {
  throw new Error("METALLIC_PLAYER_COUNT_PORT must be a valid TCP port");
}

const aggregator = new PlayerCountAggregator({ environment });
const server = createPlayerCountServer({ aggregator, ingestToken, readToken });

server.listen(port, host, () => {
  console.log(
    "Metallic player-count aggregator listening on http://" + host + ":" + port +
    " (environment " + environment + ")",
  );
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    server.close(() => process.exit(0));
    // Do not leave lingering sockets if a client stalls shutdown.
    server.closeAllConnections?.();
  });
}
