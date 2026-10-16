/**
 * Shared, cached client for the verified Metallic production status feed.
 *
 * Design goals (GitHub issue #38):
 * - Discord presence and the /players slash command always read the *same*
 *   snapshot, so both can never disagree about the real human count.
 * - Freshness is enforced from the feed's own TTL: a cached snapshot that
 *   outlives ttlSeconds becomes unavailable instead of being shown stale.
 * - Outages degrade to "unavailable" (or the last still-fresh snapshot),
 *   never to a guessed or zeroed-out count.
 * - Concurrent callers (activity timer + slash command) share one in-flight
 *   request instead of hammering the status API.
 */
import { loadProductionStatus } from "./status.mjs";

const DEFAULT_MIN_REFRESH_MS = 60000;

export class StatusFeed {
  /**
   * @param {object} options
   * @param {string} [options.apiUrl]        Verified HTTPS status URL; empty means permanently unavailable.
   * @param {string} [options.bearerToken]   Optional service-to-service token.
   * @param {number} [options.minRefreshMs]  Minimum interval between upstream fetches.
   * @param {(error: Error) => void} [options.onError] Observability hook; callers must not log secrets.
   * @param {typeof fetch} [options.fetchImpl] Injected for tests.
   */
  constructor({ apiUrl = "", bearerToken = "", minRefreshMs = DEFAULT_MIN_REFRESH_MS,
    onError = () => {}, fetchImpl = fetch } = {}) {
    if (!Number.isInteger(minRefreshMs) || minRefreshMs < 1000) {
      throw new Error("minRefreshMs must be an integer >= 1000");
    }
    this.apiUrl = apiUrl;
    this.bearerToken = bearerToken;
    this.minRefreshMs = minRefreshMs;
    this.onError = onError;
    this.fetchImpl = fetchImpl;
    this.lastSnapshot = null;
    this.lastCheckedAt = 0;
    this.inFlight = null;
    this.consecutiveFailures = 0;
  }

  /** Freshness check against the snapshot's own TTL; stale snapshots are unavailable. */
  freshAt(nowMs) {
    const status = this.lastSnapshot;
    if (!status) return null;
    if (nowMs - Date.parse(status.observedAt) > status.ttlSeconds * 1000) return null;
    return status;
  }

  /**
   * Return the shared snapshot, refreshing upstream at most once per
   * minRefreshMs. Never throws: upstream failures keep the previous snapshot
   * only while it is still inside its TTL.
   */
  async current(nowMs = Date.now()) {
    if (nowMs - this.lastCheckedAt >= this.minRefreshMs && !this.inFlight) {
      this.lastCheckedAt = nowMs;
      this.inFlight = loadProductionStatus(this.apiUrl, this.bearerToken, nowMs, this.fetchImpl)
        .then((parsed) => {
          this.consecutiveFailures = 0;
          if (parsed) this.lastSnapshot = parsed;
        })
        .catch((error) => {
          this.consecutiveFailures += 1;
          this.onError(error);
        })
        .finally(() => { this.inFlight = null; });
    }
    if (this.inFlight) await this.inFlight;
    return this.freshAt(nowMs);
  }
}
