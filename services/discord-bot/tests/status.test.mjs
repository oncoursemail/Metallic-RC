import test from "node:test";
import assert from "node:assert/strict";
import {
  parseProductionStatus, activityLabel, publicReport, requireSafeApiUrl
} from "../src/status.mjs";

const NOW = Date.parse("2026-10-15T21:00:00Z");
const GOOD = {
  environment: "production",
  source: "server-heartbeat-aggregator",
  realAuthenticatedActivePlayers: 41,
  syntheticTestPlayers: 500,
  activeRoomSessions: 9,
  ttlSeconds: 90,
  observedAt: "2026-10-15T20:59:30Z"
};

test("show real human count, never add simulated accounts", () => {
  const value = parseProductionStatus(GOOD, NOW);
  assert.equal(value.humans, 41);
  assert.equal(value.bots, 500);
  assert.equal(activityLabel(value), "Metallic · 41 online");
  assert.match(publicReport(value), /41/);
  assert.doesNotMatch(publicReport(value), /500/);
});

test("accept genuine zero count but not unavailable", () => {
  const value = parseProductionStatus({ ...GOOD, realAuthenticatedActivePlayers: 0 }, NOW);
  assert.equal(value.humans, 0);
  assert.match(publicReport(null), /not available/i);
  assert.notEqual(activityLabel(value), activityLabel(null));
});

test("reject stale, future, stage-only, invalid and fabricated data", () => {
  assert.equal(parseProductionStatus({ ...GOOD, observedAt: "2026-10-15T20:00:00Z" }, NOW), null);
  assert.equal(parseProductionStatus({ ...GOOD, observedAt: "2026-10-15T21:01:00Z" }, NOW), null);
  assert.equal(parseProductionStatus({ ...GOOD, environment: "staging" }, NOW), null);
  assert.equal(parseProductionStatus({ ...GOOD, source: "fake-client-counter" }, NOW), null);
  assert.equal(parseProductionStatus({ ...GOOD, syntheticTestPlayers: -1 }, NOW), null);
  assert.equal(parseProductionStatus({ ...GOOD, realAuthenticatedActivePlayers: "41" }, NOW), null);
  assert.equal(parseProductionStatus(null, NOW), null);
});

test("status URLs are HTTPS except local test endpoints", () => {
  assert.equal(requireSafeApiUrl("https://status.example.org/v1/aggregate"), "https://status.example.org/v1/aggregate");
  assert.equal(requireSafeApiUrl("http://127.0.0.1:4040/v1/aggregate"), "http://127.0.0.1:4040/v1/aggregate");
  assert.throws(() => requireSafeApiUrl("http://example.org/api"));
  assert.throws(() => requireSafeApiUrl("https://user:secret@example.org/path"));
  assert.throws(() => requireSafeApiUrl("not a url"));
});
