/**
 * Metallic-RC — trusted player-count aggregator HTTP surface.
 *
 * Minimal, dependency-free Node.js server around {@link PlayerCountAggregator}.
 * It is NOT deployed and does NOT receive real game telemetry yet: no
 * authoritative room server exists in this repository. It exists so the
 * ingestion/aggregation contract is real, authenticated and tested.
 *
 * Endpoints
 * ---------
 *   POST /internal/room-heartbeats   authenticated room-server heartbeat ingest
 *   GET  /v1/aggregate/players       authenticated public aggregate snapshot
 *   GET  /healthz                    unauthenticated liveness probe
 *
 * Security properties
 * -------------------
 * - Both real endpoints require a bearer service token, compared in constant
 *   time. Ingest and read tokens are distinct so a read consumer (e.g. the
 *   Discord bot) cannot publish fake counts.
 * - Bodies are size-limited and parsed strictly; malformed input is rejected.
 * - The aggregate returns only counts and a timestamp — never account ids,
 *   session ids or room names.
 */
import http from "node:http";
import crypto from "node:crypto";
import { HeartbeatError, PlayerCountAggregator } from "./aggregator.mjs";

export const INGEST_PATH = "/internal/room-heartbeats";
export const AGGREGATE_PATH = "/v1/aggregate/players";
export const HEALTH_PATH = "/healthz";

export const MAX_BODY_BYTES = 1_048_576; // 1 MiB
const MIN_TOKEN_LENGTH = 16;

const BODY_TOO_LARGE = Symbol("body-too-large");
const BODY_INVALID = Symbol("body-invalid");

const STATUS_BY_CODE = Object.freeze({
  invalid_heartbeat: 400,
  wrong_environment: 400,
  stale_heartbeat: 409,
  future_heartbeat: 409,
  out_of_order: 409,
  test_identity_in_real_sessions: 422,
  capacity_exceeded: 503,
});

/** Constant-time bearer comparison. Returns false for missing/empty input. */
export function bearerMatches(header, expected) {
  if (typeof header !== "string" || typeof expected !== "string" || expected.length === 0) {
    return false;
  }
  const match = /^Bearer (.+)$/.exec(header);
  if (!match) return false;
  const provided = Buffer.from(match[1], "utf8");
  const wanted = Buffer.from(expected, "utf8");
  if (provided.length !== wanted.length) return false;
  return crypto.timingSafeEqual(provided, wanted);
}

function respondJson(res, status, payload) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "content-length": Buffer.byteLength(body),
    "cache-control": "no-store",
  });
  res.end(body);
}

/** Read and parse a JSON body, enforcing a byte limit without resetting. */
function readJsonBody(req, limit) {
  return new Promise((resolve) => {
    const chunks = [];
    let size = 0;
    let tooLarge = false;
    let settled = false;
    const finish = (value) => {
      if (!settled) {
        settled = true;
        resolve(value);
      }
    };
    req.on("data", (chunk) => {
      size += chunk.length;
      if (size > limit) {
        tooLarge = true;
        return;
      }
      chunks.push(chunk);
    });
    req.on("end", () => {
      if (tooLarge) return finish(BODY_TOO_LARGE);
      try {
        const text = Buffer.concat(chunks).toString("utf8");
        if (text.trim().length === 0) return finish(BODY_INVALID);
        finish(JSON.parse(text));
      } catch {
        finish(BODY_INVALID);
      }
    });
    req.on("error", () => finish(BODY_INVALID));
  });
}

/**
 * Create the HTTP server. Call `.listen()` yourself (bound to loopback in
 * development and behind the protected edge in production).
 *
 * @param {{aggregator:PlayerCountAggregator, ingestToken:string, readToken?:string,
 *   logger?:{error?:Function}}} options
 */
export function createPlayerCountServer({
  aggregator,
  ingestToken,
  readToken = ingestToken,
  logger = console,
} = {}) {
  if (!(aggregator instanceof PlayerCountAggregator)) {
    throw new TypeError("A PlayerCountAggregator instance is required");
  }
  if (typeof ingestToken !== "string" || ingestToken.length < MIN_TOKEN_LENGTH) {
    throw new TypeError("A strong ingestToken (>= " + MIN_TOKEN_LENGTH + " chars) is required");
  }
  if (typeof readToken !== "string" || readToken.length < MIN_TOKEN_LENGTH) {
    throw new TypeError("A strong readToken (>= " + MIN_TOKEN_LENGTH + " chars) is required");
  }

  const authorized = (req, token) => bearerMatches(req.headers.authorization, token);

  async function handle(req, res) {
    const url = new URL(req.url ?? "/", "http://localhost");

    if (url.pathname === HEALTH_PATH) {
      if (req.method !== "GET") return respondJson(res, 405, { error: "method_not_allowed" });
      return respondJson(res, 200, { status: "ok", environment: aggregator.environment });
    }

    if (url.pathname === INGEST_PATH) {
      if (req.method !== "POST") return respondJson(res, 405, { error: "method_not_allowed" });
      if (!authorized(req, ingestToken)) {
        return respondJson(res, 401, { error: "unauthorized" });
      }
      const body = await readJsonBody(req, MAX_BODY_BYTES);
      if (body === BODY_TOO_LARGE) return respondJson(res, 413, { error: "payload_too_large" });
      if (body === BODY_INVALID) return respondJson(res, 400, { error: "invalid_json" });
      try {
        const result = aggregator.ingest(body);
        return respondJson(res, 202, { accepted: true, ...result });
      } catch (error) {
        if (error instanceof HeartbeatError) {
          const status = STATUS_BY_CODE[error.code] ?? 400;
          return respondJson(res, status, { error: error.code, detail: error.message });
        }
        throw error;
      }
    }

    if (url.pathname === AGGREGATE_PATH) {
      if (req.method !== "GET") return respondJson(res, 405, { error: "method_not_allowed" });
      if (!authorized(req, readToken)) {
        return respondJson(res, 401, { error: "unauthorized" });
      }
      const snapshot = aggregator.snapshot();
      if (!snapshot.fresh) {
        // Never serve a stale or zero-filled count as if it were live.
        return respondJson(res, 503, {
          error: "stale_no_fresh_heartbeats",
          environment: snapshot.environment,
        });
      }
      return respondJson(res, 200, snapshot);
    }

    return respondJson(res, 404, { error: "not_found" });
  }

  const server = http.createServer((req, res) => {
    handle(req, res).catch((error) => {
      logger?.error?.("player-count request failed:", error?.name ?? "Error");
      if (!res.headersSent) respondJson(res, 500, { error: "internal_error" });
      else res.destroy();
    });
  });

  server.on("clientError", (_error, socket) => {
    if (socket.writable) socket.end("HTTP/1.1 400 Bad Request\r\n\r\n");
  });

  return server;
}
