import test, { beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import { PlayerCountAggregator } from "../src/aggregator.mjs";
import { createPlayerCountServer } from "../src/server.mjs";

const NOW = Date.parse("2026-10-15T22:00:00Z");
const INGEST_TOKEN = "test-ingest-token-0123456789";
const READ_TOKEN = "test-read-token-0123456789";

let server;
let base;
let aggregator;
let clock;

beforeEach(async () => {
  clock = NOW;
  aggregator = new PlayerCountAggregator({ now: () => clock });
  server = createPlayerCountServer({
    aggregator,
    ingestToken: INGEST_TOKEN,
    readToken: READ_TOKEN,
    logger: { error() {} },
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  base = "http://127.0.0.1:" + server.address().port;
});

afterEach(async () => {
  await new Promise((resolve) => server.close(resolve));
});

const heartbeat = (overrides = {}) => ({
  schema: 1,
  source: "room-server",
  environment: "production",
  roomId: "room-alpha",
  region: "eu-west",
  observedAt: new Date(NOW).toISOString(),
  ttlSeconds: 60,
  sequence: 1,
  sessions: [],
  ...overrides,
});

const real = (accountId) => ({
  sessionId: "s-" + accountId,
  accountId,
  actorId: null,
  authenticated: true,
  synthetic: false,
});

const bot = (actorId) => ({
  sessionId: "s-" + actorId,
  actorId,
  accountId: null,
  authenticated: false,
  synthetic: true,
});

const postHeartbeat = (body, token = INGEST_TOKEN) => fetch(base + "/internal/room-heartbeats", {
  method: "POST",
  headers: {
    "content-type": "application/json",
    ...(token ? { authorization: "Bearer " + token } : {}),
  },
  body: typeof body === "string" ? body : JSON.stringify(body),
});

const readAggregate = (token = READ_TOKEN) => fetch(base + "/v1/aggregate/players", {
  headers: token ? { authorization: "Bearer " + token } : {},
});

test("health probe is public and reports the environment", async () => {
  const response = await fetch(base + "/healthz");
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { status: "ok", environment: "production" });
});

test("unknown routes and wrong methods are rejected", async () => {
  assert.equal((await fetch(base + "/nope")).status, 404);
  assert.equal((await fetch(base + "/v1/aggregate/players", { method: "POST" })).status, 405);
  assert.equal((await fetch(base + "/internal/room-heartbeats")).status, 405);
});

test("ingest and read both reject unauthorized requests", async () => {
  const body = heartbeat({ sessions: [real("acct-1")] });
  assert.equal((await postHeartbeat(body, null)).status, 401);
  assert.equal((await postHeartbeat(body, "wrong-token-000000")).status, 401);
  // The read token must not be able to publish heartbeats.
  assert.equal((await postHeartbeat(body, READ_TOKEN)).status, 401);
  // An unauthenticated read is refused.
  assert.equal((await readAggregate(null)).status, 401);
  assert.equal((await readAggregate("wrong-token-000000")).status, 401);
});

test("a valid heartbeat is accepted and the aggregate is served", async () => {
  const accepted = await postHeartbeat(heartbeat({
    sessions: [real("acct-1"), real("acct-1"), real("acct-2"), bot("testbot_run_1")],
  }));
  assert.equal(accepted.status, 202);
  assert.deepEqual(await accepted.json(), {
    accepted: true,
    roomId: "room-alpha",
    realSessions: 2,
    syntheticActors: 1,
    observedAt: new Date(NOW).toISOString(),
  });

  const response = await readAggregate();
  assert.equal(response.status, 200);
  const snapshot = await response.json();
  assert.equal(snapshot.realAuthenticatedActivePlayers, 2);
  assert.equal(snapshot.syntheticTestPlayers, 1);
  assert.equal(snapshot.activeRoomSessions, 1);
  assert.equal(snapshot.environment, "production");
  // The public aggregate must never leak player or room identities.
  const text = JSON.stringify(snapshot);
  assert.doesNotMatch(text, /acct-1|acct-2|sessionId|room-alpha/);
});

test("stale counts, bot identities and out-of-order snapshots are rejected", async () => {
  const stale = await postHeartbeat(heartbeat({
    observedAt: new Date(NOW - 61_000).toISOString(),
    ttlSeconds: 60,
  }));
  assert.equal(stale.status, 409);
  assert.equal((await stale.json()).error, "stale_heartbeat");

  const future = await postHeartbeat(heartbeat({
    observedAt: new Date(NOW + 60_000).toISOString(),
  }));
  assert.equal(future.status, 409);
  assert.equal((await future.json()).error, "future_heartbeat");

  const testIdentity = await postHeartbeat(heartbeat({
    sessions: [real("testbot_run_7")],
  }));
  assert.equal(testIdentity.status, 422);
  assert.equal((await testIdentity.json()).error, "test_identity_in_real_sessions");

  assert.equal((await postHeartbeat(heartbeat({ sequence: 5 }))).status, 202);
  const replayed = await postHeartbeat(heartbeat({ sequence: 4 }));
  assert.equal(replayed.status, 409);
  assert.equal((await replayed.json()).error, "out_of_order");

  const staged = await postHeartbeat(heartbeat({ environment: "staging" }));
  assert.equal(staged.status, 400);
  assert.equal((await staged.json()).error, "wrong_environment");
});

test("malformed and oversized bodies are rejected", async () => {
  assert.equal((await postHeartbeat("not json")).status, 400);
  const tooBig = JSON.stringify({ padding: "x".repeat(1_100_000) });
  assert.equal((await postHeartbeat(tooBig)).status, 413);
});

test("read reports unavailable instead of a stale or zero count", async () => {
  assert.equal((await readAggregate()).status, 503);
  assert.equal((await (await readAggregate()).json()).error, "stale_no_fresh_heartbeats");

  await postHeartbeat(heartbeat({ ttlSeconds: 30, sessions: [real("acct-1")] }));
  assert.equal((await readAggregate()).status, 200);

  // Advance past the heartbeat TTL: the count must disappear, not read as 0.
  clock = NOW + 31_000;
  const response = await readAggregate();
  assert.equal(response.status, 503);
  assert.equal((await response.json()).error, "stale_no_fresh_heartbeats");
});

test("the server refuses to start without strong distinct tokens", () => {
  assert.throws(() => createPlayerCountServer({ aggregator, ingestToken: "short" }));
  assert.throws(() => createPlayerCountServer({ aggregator, ingestToken: INGEST_TOKEN, readToken: "short" }));
  assert.throws(() => createPlayerCountServer({ ingestToken: INGEST_TOKEN, readToken: READ_TOKEN }));
});
