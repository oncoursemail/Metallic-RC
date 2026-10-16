/**
 * End-to-end tests for the verified status-feed client (GitHub issue #38).
 * A real loopback HTTP server stands in for the production status API so the
 * full path — bearer token, JSON validation, TTL freshness, outage fallback,
 * request dedupe — is exercised, not mocked away.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { once } from "node:events";
import { StatusFeed } from "../src/feed.mjs";
import { activityLabel, publicReport } from "../src/status.mjs";

function startStatusServer(handler) {
  const server = createServer(handler);
  server.listen(0, "127.0.0.1");
  return once(server, "listening").then(() => ({
    url: "http://127.0.0.1:" + server.address().port + "/v1/aggregate",
    close: () => new Promise((resolve) => server.close(resolve)),
  }));
}

function payload(overrides = {}, nowMs = Date.now()) {
  return {
    environment: "production",
    source: "server-heartbeat-aggregator",
    realAuthenticatedActivePlayers: 42,
    syntheticTestPlayers: 250,
    activeRoomSessions: 3,
    observedAt: new Date(nowMs - 1000).toISOString(),
    ttlSeconds: 60,
    ...overrides,
  };
}

test("presence and /players read one shared snapshot with the same real count", async () => {
  let requests = 0;
  const seenAuth = [];
  const srv = await startStatusServer((req, res) => {
    requests += 1;
    seenAuth.push(req.headers.authorization || null);
    res.setHeader("Content-Type", "application/json");
    res.end(JSON.stringify(payload()));
  });
  try {
    const feed = new StatusFeed({ apiUrl: srv.url, bearerToken: "svc-secret-token" });
    const forPresence = await feed.current();
    const forCommand = await feed.current();
    assert.equal(forPresence.humans, 42);
    assert.equal(forPresence, forCommand, "slash command must reuse the presence snapshot");
    assert.equal(activityLabel(forPresence), "Metallic · 42 online");
    assert.match(publicReport(forCommand), /\*\*42\*\*/);
    assert.doesNotMatch(publicReport(forCommand), /250/, "synthetic actors never shown");
    assert.equal(requests, 1, "second read is cached, not re-fetched");
    assert.deepEqual(seenAuth, ["Bearer svc-secret-token"]);
  } finally { await srv.close(); }
});

test("unconfigured feed reports unavailable instead of zero players", async () => {
  const feed = new StatusFeed({ apiUrl: "" });
  assert.equal(await feed.current(), null);
  assert.match(publicReport(await feed.current()), /not available/i);
  assert.equal(activityLabel(null), "Metallic · status unavailable");
});

test("outage keeps the last snapshot only inside its TTL, then says unavailable", async () => {
  let failing = false;
  let count = 42;
  const srv = await startStatusServer((req, res) => {
    if (failing) { res.statusCode = 503; res.end("maintenance"); return; }
    res.setHeader("Content-Type", "application/json");
    res.end(JSON.stringify(payload({ realAuthenticatedActivePlayers: count }, Date.now())));
  });
  try {
    const errors = [];
    const feed = new StatusFeed({
      apiUrl: srv.url,
      minRefreshMs: 1000,
      onError: (error) => errors.push(error.name),
    });
    assert.equal((await feed.current()).humans, 42);

    // Short outage: within TTL the last good snapshot is served.
    failing = true;
    await new Promise((r) => setTimeout(r, 1100));
    assert.equal((await feed.current()).humans, 42, "fresh cache survives a failed refresh");

    // Beyond the TTL the count is stale; unavailable beats a wrong number.
    feed.lastSnapshot.observedAt = new Date(Date.now() - 61000).toISOString();
    await new Promise((r) => setTimeout(r, 1100));
    assert.equal(await feed.current(), null, "stale snapshot must not be shown");
    assert.equal(activityLabel(await feed.current()), "Metallic · status unavailable");
    assert.ok(errors.length >= 1, "failures are reported to the observability hook");
  } finally { await srv.close(); }
});

test("rejected payloads (staging, synthetic counters) never replace the snapshot", async () => {
  let body = { environment: "staging", source: "server-heartbeat-aggregator",
    realAuthenticatedActivePlayers: 9999, syntheticTestPlayers: 0,
    observedAt: new Date().toISOString(), ttlSeconds: 60 };
  const srv = await startStatusServer((req, res) => {
    res.setHeader("Content-Type", "application/json");
    res.end(JSON.stringify(body));
  });
  try {
    const feed = new StatusFeed({ apiUrl: srv.url, minRefreshMs: 1000 });
    assert.equal(await feed.current(), null, "staging data is unavailable");

    body = payload({ environment: "production", source: "fake-client-counter" });
    await new Promise((r) => setTimeout(r, 1100));
    assert.equal(await feed.current(), null, "non-heartbeat sources are unavailable");

    body = payload();
    await new Promise((r) => setTimeout(r, 1100));
    assert.equal((await feed.current()).humans, 42, "valid production data recovers");
  } finally { await srv.close(); }
});

test("concurrent readers share one in-flight upstream request", async () => {
  let requests = 0;
  const srv = await startStatusServer(async (req, res) => {
    requests += 1;
    await new Promise((r) => setTimeout(r, 100));
    res.setHeader("Content-Type", "application/json");
    res.end(JSON.stringify(payload()));
  });
  try {
    const feed = new StatusFeed({ apiUrl: srv.url });
    const [a, b, c] = await Promise.all([feed.current(), feed.current(), feed.current()]);
    assert.equal(requests, 1);
    assert.equal(a, b);
    assert.equal(b, c);
  } finally { await srv.close(); }
});

test("minRefreshMs must be a sane integer", () => {
  assert.throws(() => new StatusFeed({ minRefreshMs: 10 }));
  assert.throws(() => new StatusFeed({ minRefreshMs: -1 }));
  assert.doesNotThrow(() => new StatusFeed({}));
});
