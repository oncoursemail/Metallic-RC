/**
 * Metallic-RC — trusted production player-count aggregator (core logic).
 *
 * This is the SERVER-SIDE component that turns authenticated room-server
 * heartbeats into the aggregate consumed by the Discord bot and future status
 * dashboards. It is intentionally pure and dependency-free so it can be unit
 * tested deterministically and embedded in a modular backend later.
 *
 * Trust model
 * -----------
 * - Heartbeats are produced by authenticated room servers and delivered over an
 *   authenticated channel (see server.mjs). This module validates the *shape*
 *   and *semantics* of an already-authenticated payload; it never talks to a
 *   game client and never accepts player-supplied roles, counts or identities.
 * - Only authenticated, non-synthetic sessions count as real humans.
 * - Synthetic load actors (`testbot_*` namespace) are tracked separately and can
 *   never contribute to the public player count.
 * - Every heartbeat is a full snapshot of a room, so repeated heartbeats cannot
 *   inflate a count. Real players are de-duplicated by account id across rooms
 *   and devices.
 * - Counts expire with a TTL, so a crashed or silent room stops contributing.
 *
 * This module does NOT prove the numbers are real: server authenticity comes
 * from the authenticated transport and secure service-to-service tokens, not
 * from string validation alone.
 */

export const HEARTBEAT_SCHEMA = 1;
export const TEST_ACTOR_PREFIX = "testbot_";
export const HEARTBEAT_SOURCE = "room-server";
export const AGGREGATE_SOURCE = "server-heartbeat-aggregator";

/** A heartbeat timestamped further in the future than this is rejected. */
export const MAX_CLOCK_SKEW_MS = 30_000;
export const MIN_TTL_SECONDS = 10;
export const MAX_TTL_SECONDS = 300;

const MAX_SESSIONS_PER_HEARTBEAT = 10_000;
const MAX_IDENTIFIER_LENGTH = 128;
const DEFAULT_MAX_ROOMS = 10_000;
const DEFAULT_SNAPSHOT_TTL_SECONDS = 60;

/**
 * @typedef {object} Session
 * @property {string} sessionId
 * @property {boolean} authenticated
 * @property {boolean} synthetic
 * @property {string|null} [accountId]
 * @property {string|null} [actorId]
 */

/** Error carrying a stable machine-readable rejection code. */
export class HeartbeatError extends Error {
  /** @param {string} code @param {string} message */
  constructor(code, message) {
    super(message);
    this.name = "HeartbeatError";
    this.code = code;
  }
}

function invalid(message) {
  return new HeartbeatError("invalid_heartbeat", message);
}

function isPlainObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function requireIdentifier(value, field) {
  if (typeof value !== "string" || value.length < 1 || value.length > MAX_IDENTIFIER_LENGTH) {
    throw invalid(field + " must be a non-empty string up to " + MAX_IDENTIFIER_LENGTH + " characters");
  }
  return value;
}

/**
 * Validate one session and add its identity to the appropriate set.
 * Throws HeartbeatError for anything that could inflate the real count.
 */
function collectSession(session, accounts, actors) {
  if (!isPlainObject(session)) throw invalid("Each session must be an object");
  requireIdentifier(session.sessionId, "sessionId");
  if (typeof session.synthetic !== "boolean") {
    throw invalid("session.synthetic must be a boolean");
  }

  if (session.synthetic) {
    const actorId = session.actorId;
    if (typeof actorId !== "string" || !actorId.startsWith(TEST_ACTOR_PREFIX) ||
        actorId.length > MAX_IDENTIFIER_LENGTH) {
      throw invalid("Synthetic session must use a " + TEST_ACTOR_PREFIX + " actorId");
    }
    if (session.accountId != null) {
      throw invalid("Synthetic session must not carry a real accountId");
    }
    if (session.authenticated === true) {
      throw invalid("Synthetic session must not claim real authentication");
    }
    actors.add(actorId);
    return;
  }

  // Real session: authentication is mandatory and test identities are refused.
  if (session.authenticated !== true) {
    throw invalid("Real session must be authenticated");
  }
  if (session.actorId != null) {
    throw new HeartbeatError(
      "test_identity_in_real_sessions",
      "Synthetic actor id present in a real session",
    );
  }
  const accountId = requireIdentifier(session.accountId, "accountId");
  if (accountId.startsWith(TEST_ACTOR_PREFIX)) {
    throw new HeartbeatError(
      "test_identity_in_real_sessions",
      "Test identity cannot be counted as a real authenticated player",
    );
  }
  accounts.add(accountId);
}

/**
 * Parse and fully validate a heartbeat. Assumes the caller already
 * authenticated the transport.
 * @returns {{roomId:string, region:string|null, observedMs:number, observedAt:string,
 *   ttlSeconds:number, sequence:number, accounts:Set<string>, actors:Set<string>}}
 */
export function parseHeartbeat(raw, environment, nowMs, maxClockSkewMs = MAX_CLOCK_SKEW_MS) {
  if (!isPlainObject(raw)) throw invalid("Heartbeat must be a JSON object");
  if (raw.schema !== HEARTBEAT_SCHEMA) throw invalid("Unsupported heartbeat schema");
  if (raw.source !== HEARTBEAT_SOURCE) {
    throw invalid("Heartbeat source must be an authenticated room server");
  }
  if (raw.environment !== environment) {
    throw new HeartbeatError(
      "wrong_environment",
      "Heartbeat environment is not " + environment,
    );
  }

  const roomId = requireIdentifier(raw.roomId, "roomId");
  const region = raw.region == null ? null : requireIdentifier(raw.region, "region");

  if (typeof raw.observedAt !== "string") throw invalid("observedAt is required");
  const observedMs = Date.parse(raw.observedAt);
  if (!Number.isFinite(observedMs)) throw invalid("observedAt is not a valid timestamp");
  if (observedMs - nowMs > maxClockSkewMs) {
    throw new HeartbeatError("future_heartbeat", "Heartbeat timestamp is in the future");
  }

  const ttlSeconds = raw.ttlSeconds;
  if (!Number.isInteger(ttlSeconds) || ttlSeconds < MIN_TTL_SECONDS || ttlSeconds > MAX_TTL_SECONDS) {
    throw invalid("ttlSeconds must be an integer between " +
      MIN_TTL_SECONDS + " and " + MAX_TTL_SECONDS);
  }
  if (nowMs - observedMs > ttlSeconds * 1000) {
    throw new HeartbeatError("stale_heartbeat", "Heartbeat is older than its TTL");
  }

  const sequence = raw.sequence;
  if (!Number.isInteger(sequence) || sequence < 0) {
    throw invalid("sequence must be a non-negative integer");
  }
  if (!Array.isArray(raw.sessions)) throw invalid("sessions must be an array");
  if (raw.sessions.length > MAX_SESSIONS_PER_HEARTBEAT) {
    throw invalid("sessions exceeds the per-heartbeat limit");
  }

  const accounts = new Set();
  const actors = new Set();
  for (const session of raw.sessions) collectSession(session, accounts, actors);

  return { roomId, region, observedMs, observedAt: raw.observedAt, ttlSeconds, sequence, accounts, actors };
}

function isFresh(room, nowMs) {
  return nowMs - room.observedMs <= room.ttlSeconds * 1000;
}

function isNewer(next, current) {
  if (next.observedMs !== current.observedMs) return next.observedMs > current.observedMs;
  return next.sequence > current.sequence;
}

/** In-memory aggregator. Replace the store with a durable one when it ships. */
export class PlayerCountAggregator {
  #rooms = new Map();
  #environment;
  #now;
  #maxClockSkewMs;
  #maxRooms;
  #snapshotTtlSeconds;

  constructor({
    environment = "production",
    now = () => Date.now(),
    maxClockSkewMs = MAX_CLOCK_SKEW_MS,
    maxRooms = DEFAULT_MAX_ROOMS,
    snapshotTtlSeconds = DEFAULT_SNAPSHOT_TTL_SECONDS,
  } = {}) {
    if (typeof environment !== "string" || environment.length === 0) {
      throw new TypeError("environment is required");
    }
    if (typeof now !== "function") throw new TypeError("now must be a function");
    if (!Number.isInteger(maxRooms) || maxRooms < 1) throw new TypeError("maxRooms must be a positive integer");
    if (!Number.isInteger(snapshotTtlSeconds) || snapshotTtlSeconds < MIN_TTL_SECONDS ||
        snapshotTtlSeconds > MAX_TTL_SECONDS) {
      throw new TypeError("snapshotTtlSeconds must be an integer between " +
        MIN_TTL_SECONDS + " and " + MAX_TTL_SECONDS);
    }
    this.#environment = environment;
    this.#now = now;
    this.#maxClockSkewMs = maxClockSkewMs;
    this.#maxRooms = maxRooms;
    this.#snapshotTtlSeconds = snapshotTtlSeconds;
  }

  get environment() {
    return this.#environment;
  }

  /** Number of room snapshots currently held (including any stale ones). */
  get roomCount() {
    return this.#rooms.size;
  }

  /**
   * Accept an authenticated heartbeat. Throws {@link HeartbeatError} when the
   * payload is malformed, stale, out of order or contains bot/test identities.
   */
  ingest(raw) {
    const nowMs = this.#now();
    const heartbeat = parseHeartbeat(raw, this.#environment, nowMs, this.#maxClockSkewMs);
    const current = this.#rooms.get(heartbeat.roomId);
    if (current && !isNewer(heartbeat, current)) {
      throw new HeartbeatError(
        "out_of_order",
        "Heartbeat is not newer than the last accepted snapshot for this room",
      );
    }
    if (!current) {
      this.prune();
      if (this.#rooms.size >= this.#maxRooms) {
        throw new HeartbeatError("capacity_exceeded", "Aggregator room capacity reached");
      }
    }
    this.#rooms.set(heartbeat.roomId, heartbeat);
    return {
      roomId: heartbeat.roomId,
      realSessions: heartbeat.accounts.size,
      syntheticActors: heartbeat.actors.size,
      observedAt: new Date(heartbeat.observedMs).toISOString(),
    };
  }

  /** Drop room snapshots whose TTL has elapsed. */
  prune() {
    const nowMs = this.#now();
    for (const [roomId, room] of this.#rooms) {
      if (!isFresh(room, nowMs)) this.#rooms.delete(roomId);
    }
  }

  /**
   * Build the public aggregate. Real players are de-duplicated by account id
   * across every fresh room; synthetic actors are counted independently.
   * `fresh` is false when no room is currently within its TTL.
   */
  snapshot() {
    const nowMs = this.#now();
    const rooms = [...this.#rooms.values()].filter((room) => isFresh(room, nowMs));

    if (rooms.length === 0) {
      return Object.freeze({
        schema: HEARTBEAT_SCHEMA,
        environment: this.#environment,
        fresh: false,
        realAuthenticatedActivePlayers: 0,
        syntheticTestPlayers: 0,
        activeRoomSessions: 0,
        observedAt: null,
        ttlSeconds: 0,
        source: AGGREGATE_SOURCE,
      });
    }

    const accounts = new Set();
    const actors = new Set();
    for (const room of rooms) {
      for (const accountId of room.accounts) accounts.add(accountId);
      for (const actorId of room.actors) actors.add(actorId);
    }

    return Object.freeze({
      schema: HEARTBEAT_SCHEMA,
      environment: this.#environment,
      fresh: true,
      realAuthenticatedActivePlayers: accounts.size,
      syntheticTestPlayers: actors.size,
      activeRoomSessions: rooms.length,
      // The snapshot is fresh by construction: it was assembled now from
      // rooms that are all within their own TTL.
      observedAt: new Date(nowMs).toISOString(),
      ttlSeconds: this.#snapshotTtlSeconds,
      source: AGGREGATE_SOURCE,
    });
  }
}
