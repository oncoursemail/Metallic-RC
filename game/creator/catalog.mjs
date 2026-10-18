/**
 * Metallic creator catalog contract prototype (NOT a deployed asset store).
 * Only trusted, already-moderated metadata may be passed into this module.
 * Actual package scanning, approval, signatures, access control and payments
 * must be enforced by a server-side publishing pipeline.
 */
export const INVENTION_CATEGORIES = Object.freeze([
  "animations", "vfx", "buildings", "circuits", "props", "audio",
  "materials", "characters", "ui", "worlds", "templates", "pets", "tools",
]);
export const CONTENT_RATINGS = Object.freeze(["everyone", "teen", "adult"]);
const RATINGS = new Map(CONTENT_RATINGS.map((rating, level) => [rating, level]));
const STATUSES = new Set(["draft", "in_review", "approved", "rejected", "delisted"]);
const ID = /^[a-z0-9][a-z0-9._-]{2,63}$/i;
const TAG = /^[a-z0-9][a-z0-9-]{0,23}$/;
const HEX_SHA = /^[a-f0-9]{64}$/;
const isObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
function requireValid(ok, message) { if (!ok) throw new TypeError(message); }
function identifier(value) { return typeof value === "string" && ID.test(value); }

/**
 * Validation is intentionally narrow. A complete publishing service must also
 * scan content for malware, assets/license abuse, performance and safety.
 */
export function validateInvention(raw) {
  requireValid(isObject(raw), "Invention must be an object");
  requireValid(raw.schema === 1, "Unsupported invention schema");
  requireValid(identifier(raw.id), "Invalid invention id");
  requireValid(identifier(raw.authorId), "Invalid creator id");
  requireValid(typeof raw.title === "string" && raw.title === raw.title.trim() &&
    raw.title.length >= 3 && raw.title.length <= 90, "Invalid invention title");
  requireValid(typeof raw.description === "string" && raw.description.length <= 500,
    "Invalid description");
  requireValid(INVENTION_CATEGORIES.includes(raw.category), "Invalid category");
  requireValid(CONTENT_RATINGS.includes(raw.rating), "Invalid content rating");
  requireValid(STATUSES.has(raw.status), "Invalid moderation state");
  requireValid(/^[a-z0-9][a-z0-9._-]{0,63}$/i.test(raw.revision), "Invalid revision");
  requireValid(Array.isArray(raw.tags) && raw.tags.length <= 12 &&
    raw.tags.every((t) => typeof t === "string" && TAG.test(t)) &&
    new Set(raw.tags).size === raw.tags.length, "Invalid or duplicate tags");
  requireValid(Array.isArray(raw.files) && raw.files.length >= 1 && raw.files.length <= 24,
    "Expected 1–24 content files");
  for (const file of raw.files) {
    requireValid(isObject(file) && HEX_SHA.test(file.sha256) &&
      Number.isSafeInteger(file.bytes) && file.bytes > 0 && file.bytes <= 50_000_000,
      "Invalid content file digest/size");
  }
  // Approval must be attached to precisely this immutable revision, not an
  // earlier version of the same id that could have been changed by a creator.
  if (raw.status === "approved") {
    requireValid(raw.approvedRevision === raw.revision && raw.licenseVerified === true &&
      typeof raw.approvedAt === "string" &&
      Number.isFinite(Date.parse(raw.approvedAt)), "Missing revision-bound approval");
  }
  return Object.freeze({
    schema: 1, id: raw.id, authorId: raw.authorId, title: raw.title,
    description: raw.description, category: raw.category, rating: raw.rating,
    status: raw.status, revision: raw.revision, tags: Object.freeze([...raw.tags]),
    files: Object.freeze(raw.files.map((f) => Object.freeze({ sha256: f.sha256, bytes: f.bytes }))),
    approvedRevision: raw.approvedRevision ?? null,
    licenseVerified: raw.licenseVerified === true,
    approvedAt: raw.approvedAt ?? null,
  });
}

/** Search approved metadata; default audience is everyone (fail-closed). */
export function searchInventions(source, {
  query = "", category = null, requiredTags = [], maxRating = "everyone",
  sort = "relevance", limit = 20, offset = 0,
} = {}) {
  requireValid(Array.isArray(source), "Expected invention array");
  requireValid(typeof query === "string" && query.length <= 120, "Invalid query");
  requireValid(category === null || INVENTION_CATEGORIES.includes(category), "Invalid category filter");
  requireValid(CONTENT_RATINGS.includes(maxRating), "Invalid audience rating");
  requireValid(Array.isArray(requiredTags) && requiredTags.length <= 12 &&
    requiredTags.every((tag) => typeof tag === "string" && TAG.test(tag)),
    "Invalid tag filters");
  requireValid(["relevance", "newest", "title"].includes(sort), "Invalid sort");
  requireValid(Number.isSafeInteger(limit) && limit >= 1 && limit <= 50, "Invalid limit");
  requireValid(Number.isSafeInteger(offset) && offset >= 0 && offset <= 10000, "Invalid offset");
  const ids = new Set();
  const needle = query.trim().toLocaleLowerCase("en-US");
  const matches = [];
  for (const data of source) {
    const item = validateInvention(data);
    requireValid(!ids.has(item.id), "Duplicate invention id");
    ids.add(item.id);
    if (item.status !== "approved" || item.approvedRevision !== item.revision ||
        !item.licenseVerified || RATINGS.get(item.rating) > RATINGS.get(maxRating)) continue;
    if (category && category !== item.category) continue;
    if (!requiredTags.every((tag) => item.tags.includes(tag))) continue;
    const title = item.title.toLocaleLowerCase("en-US");
    const tags = item.tags.join(" ");
    const text = (item.description + " " + item.category + " " + tags).toLocaleLowerCase("en-US");
    if (needle && !title.includes(needle) && !text.includes(needle)) continue;
    const score = needle ? (title === needle ? 100 : title.startsWith(needle) ? 50 :
      title.includes(needle) ? 20 : tags.includes(needle) ? 15 : 5) : 0;
    // Public results intentionally exclude file hashes and future download URLs.
    matches.push({ id: item.id, title: item.title, description: item.description,
      category: item.category, rating: item.rating, authorId: item.authorId,
      tags: [...item.tags], revision: item.revision, approvedAt: item.approvedAt, score });
  }
  matches.sort((a, b) => {
    if (sort === "newest") return b.approvedAt.localeCompare(a.approvedAt) || a.id.localeCompare(b.id);
    if (sort === "relevance") return b.score - a.score || a.title.localeCompare(b.title) || a.id.localeCompare(b.id);
    return a.title.localeCompare(b.title) || a.id.localeCompare(b.id);
  });
  return { total: matches.length, results: matches.slice(offset, offset + limit).map(({ score, ...item }) => item) };
}
