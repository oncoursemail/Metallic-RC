/**
 * Authoritative aggregate player-count validation for the Metallic Discord bot.
 * Never invent online counts or count synthetic load actors as real users.
 */
export function parseProductionStatus(value, nowMs = Date.now()) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  if (value.environment !== "production" || value.source !== "server-heartbeat-aggregator") return null;
  const count = value.realAuthenticatedActivePlayers;
  if (!Number.isSafeInteger(count) || count < 0 || count > 10000000) return null;
  const bots = value.syntheticTestPlayers;
  if (!Number.isSafeInteger(bots) || bots < 0) return null;
  if (typeof value.observedAt !== "string") return null;
  const stamp = Date.parse(value.observedAt);
  if (!Number.isFinite(stamp) || stamp - nowMs > 30000) return null;
  const ttl = value.ttlSeconds;
  if (!Number.isInteger(ttl) || ttl < 10 || ttl > 300) return null;
  if (nowMs - stamp > ttl * 1000) return null;
  return {
    humans: count,
    bots,
    observedAt: value.observedAt,
    rooms: Number.isSafeInteger(value.activeRoomSessions) && value.activeRoomSessions >= 0
      ? value.activeRoomSessions : null,
  };
}

export function activityLabel(status) {
  return status ? "Metallic · " + status.humans.toLocaleString("en-US") + " online"
    : "Metallic · status unavailable";
}

export function publicReport(status) {
  if (!status) return "Metallic production player count is not available. No verified live feed has been configured.";
  const rooms = status.rooms !== null ? "\nActive room sessions: **" + status.rooms + "**" : "";
  return "Verified active players: **" + status.humans.toLocaleString("en-US") +
    "**" + rooms + "\nSample: " + status.observedAt +
    "\n(Synthetic tests excluded; server heartbeat snapshot.)";
}

export function requireSafeApiUrl(raw) {
  if (!raw) return null;
  let url;
  try { url = new URL(raw); } catch { throw new Error("Malformed METALLIC_STATUS_URL"); }
  if (url.protocol !== "https:" && !(url.protocol === "http:" &&
      ["127.0.0.1", "localhost", "::1", "[::1]"].includes(url.hostname))) {
    throw new Error("Status API URL must be HTTPS, except for local development");
  }
  if (url.username || url.password || url.hash) throw new Error("Do not put credentials/fragments in status URL");
  return url.toString();
}

export async function loadProductionStatus(apiUrl, token, nowMs = Date.now()) {
  const url = requireSafeApiUrl(apiUrl);
  if (!url) return null;
  const headers = { "Accept": "application/json" };
  if (token) headers.Authorization = "Bearer " + token;
  const response = await fetch(url, {
    method: "GET", headers, redirect: "error", signal: AbortSignal.timeout(4000),
  });
  if (!response.ok) throw new Error("Metallic status API unavailable (" + response.status + ")");
  const payload = await response.json();
  return parseProductionStatus(payload, nowMs);
}
