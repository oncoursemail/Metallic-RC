import test from "node:test";
import assert from "node:assert/strict";
import {
  PlayerCountAggregator,
  HeartbeatError,
  TEST_ACTOR_PREFIX,
} from "../src/aggregator.mjs";
import { parseProductionStatus } from "../../discord-bot/src/status.mjs";

const NOW = Date.parse("2026-10-15T22:00:00Z");

/** Full-room heartbeat with overridable fields. */
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

const real = (accountId, extra = {}) => ({
  sessionId: "s-" + accountId,
  accountId,
  actorId: null,
  authenticated: true,
  synthetic: false,
  ...extra,
});

const bot = (actorId, extra = {}) => ({
  sessionId: "s-" + actorId,
  actorId,
  accountId: null,
  authenticated: false,
  synthetic: true,
  ...extra,
});

/** Aggregator with a controllable clock. */
function harness(options = {}) {
  let clock = NOW;
  const aggregator = new PlayerCountAggregator({ now: () => clock, ...options });
  return { aggregator, advance: (ms) => { clock += ms; }, setClock: (ms) => { clock = ms; } };
}

const assertCode = (code, fn) => {
  assert.throws(fn, (error) => {
    assert.ok(error instanceof HeartbeatError, "expected HeartbeatError");
    assert.equal(error.code, code);
    return true;
  });
};

test("counts each authenticated account once across duplicate sessions and devices", () => {
  const { aggregator } = harness();
  aggregator.ingest(heartbeat({
    sessions: [
      real("acct-1"),
      real("acct-1"), // duplicate session in the same snapshot
      real("acct-1", { sessionId: "s-acct-1-quest", deviceId: "quest-3" }), // second device
      real("acct-2"),
    ],
  }));

  const snapshot = aggregator.snapshot();
  assert.equal(snapshot.fresh, true);
  assert.equal(snapshot.realAuthenticatedActivePlayers, 2);
  assert.equal(snapshot.syntheticTestPlayers, 0);
  assert.equal(snapshot.activeRoomSessions, 1);
  assert.equal(snapshot.source, "server-heartbeat-aggregator");
  assert.match(snapshot.observedAt, /^\d{4}-\d{2}-\d{2}T/);
});

test("de-duplicates one account present in several rooms", () => {
  const { aggregator } = harness();
  aggregator.ingest(heartbeat({ roomId: "room-a", sessions: [real("acct-1")] }));
  aggregator.ingest(heartbeat({ roomId: "room-b", sequence: 7, sessions: [real("acct-1"), real("acct-2")] }));

  const snapshot = aggregator.snapshot();
  assert.equal(snapshot.realAuthenticatedActivePlayers, 2);
  assert.equal(snapshot.activeRoomSessions, 2);
});

test("tracks synthetic test actors separately and never counts them as real", () => {
  const { aggregator } = harness();
  aggregator.ingest(heartbeat({
    sessions: [
      real("acct-1"),
      bot("testbot_run_1"),
      bot("testbot_run_2"),
      bot("testbot_run_2"), // duplicate actor in one snapshot
    ],
  }));

  const snapshot = aggregator.snapshot();
  assert.equal(snapshot.realAuthenticatedActivePlayers, 1);
  assert.equal(snapshot.syntheticTestPlayers, 2);
});

test("room counts expire with the heartbeat TTL", () => {
  const { aggregator, advance } = harness();
  aggregator.ingest(heartbeat({ ttlSeconds: 30, sessions: [real("acct-1")] }));
  assert.equal(aggregator.snapshot().fresh, true);

  advance(30_001);
  const stale = aggregator.snapshot();
  assert.equal(stale.fresh, false);
  assert.equal(stale.realAuthenticatedActivePlayers, 0);
  assert.equal(stale.observedAt, null);
});

test("prunes expired rooms instead of leaking memory", () => {
  const { aggregator, advance } = harness();
  aggregator.ingest(heartbeat({ roomId: "room-a", sessions: [real("acct-1")] }));
  aggregator.ingest(heartbeat({ roomId: "room-b", sessions: [real("acct-2")] }));
  assert.equal(aggregator.roomCount, 2);

  advance(61_000);
  aggregator.prune();
  assert.equal(aggregator.roomCount, 0);
});

test("rejects stale, future and out-of-order heartbeats", () => {
  const { aggregator } = harness();

  assertCode("stale_heartbeat", () => aggregator.ingest(heartbeat({
    observedAt: new Date(NOW - 61_000).toISOString(),
    ttlSeconds: 60,
  })));
  assertCode("future_heartbeat", () => aggregator.ingest(heartbeat({
    observedAt: new Date(NOW + 60_000).toISOString(),
  })));

  aggregator.ingest(heartbeat({ sequence: 5 }));
  // Older timestamp for the same room is refused.
  assertCode("out_of_order", () => aggregator.ingest(heartbeat({
    sequence: 9,
    observedAt: new Date(NOW - 1_000).toISOString(),
  })));
  // Same timestamp with a non-increasing sequence is refused.
  assertCode("out_of_order", () => aggregator.ingest(heartbeat({ sequence: 5 })));
});

test("rejects bot and test identities claiming to be real players", () => {
  const { aggregator } = harness();

  assertCode("test_identity_in_real_sessions", () => aggregator.ingest(heartbeat({
    sessions: [real(TEST_ACTOR_PREFIX + "run_1")],
  })));
  assertCode("test_identity_in_real_sessions", () => aggregator.ingest(heartbeat({
    sessions: [real("acct-1", { actorId: TEST_ACTOR_PREFIX + "sneaky" })],
  })));
  // A synthetic session must not impersonate a real account or real auth.
  assertCode("invalid_heartbeat", () => aggregator.ingest(heartbeat({
    sessions: [bot("testbot_run_1", { accountId: "acct-1" })],
  })));
  assertCode("invalid_heartbeat", () => aggregator.ingest(heartbeat({
    sessions: [bot("testbot_run_1", { authenticated: true })],
  })));
  // A synthetic session must use the reserved namespace.
  assertCode("invalid_heartbeat", () => aggregator.ingest(heartbeat({
    sessions: [{ sessionId: "s-1", actorId: "alice", authenticated: false, synthetic: true }],
  })));
});

test("rejects unauthenticated, malformed and wrong-environment heartbeats", () => {
  const { aggregator } = harness();

  assertCode("invalid_heartbeat", () => aggregator.ingest(heartbeat({
    sessions: [real("acct-1", { authenticated: false })],
  })));
  assertCode("invalid_heartbeat", () => aggregator.ingest(heartbeat({ schema: 2 })));
  assertCode("invalid_heartbeat", () => aggregator.ingest(heartbeat({ source: "fake-client-counter" })));
  assertCode("invalid_heartbeat", () => aggregator.ingest(heartbeat({ sessions: "none" })));
  assertCode("invalid_heartbeat", () => aggregator.ingest(heartbeat({ ttlSeconds: 5 })));
  assertCode("invalid_heartbeat", () => aggregator.ingest(heartbeat({ roomId: "" })));
  assertCode("invalid_heartbeat", () => aggregator.ingest(heartbeat({ observedAt: "not-a-date" })));
  assertCode("invalid_heartbeat", () => aggregator.ingest(heartbeat({ sequence: -1 })));
  assertCode("invalid_heartbeat", () => aggregator.ingest(null));
  assertCode("wrong_environment", () => aggregator.ingest(heartbeat({ environment: "staging" })));
});

test("refuses new rooms once the configured capacity is reached", () => {
  const { aggregator } = harness({ maxRooms: 1 });
  aggregator.ingest(heartbeat({ roomId: "room-a" }));
  assertCode("capacity_exceeded", () => aggregator.ingest(heartbeat({ roomId: "room-b" })));
});

test("aggregate snapshot satisfies the Discord status contract exactly", () => {
  const { aggregator } = harness();
  aggregator.ingest(heartbeat({
    sessions: [real("acct-1"), real("acct-2"), bot("testbot_run_9")],
  }));

  const parsed = parseProductionStatus(aggregator.snapshot(), NOW);
  assert.ok(parsed, "the Discord validator must accept the aggregate");
  assert.equal(parsed.humans, 2);
  assert.equal(parsed.bots, 1);
  assert.equal(parsed.rooms, 1);
});

test("a fully stale snapshot is reported unavailable, never as zero online", () => {
  const { aggregator, advance } = harness();
  aggregator.ingest(heartbeat({ ttlSeconds: 30, sessions: [real("acct-1")] }));
  advance(31_000);

  const snapshot = aggregator.snapshot();
  assert.equal(snapshot.fresh, false);
  assert.equal(parseProductionStatus(snapshot, NOW), null);
});
