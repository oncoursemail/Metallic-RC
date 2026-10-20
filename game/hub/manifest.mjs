/**
 * Declarative original social-hub specification and watch-map projection.
 * A manifest is NOT a game scene: artists/build engineers must produce
 * meshes, lighting, collision, navmesh, audio and authoritative interactions.
 */
const ID = /^[a-z0-9][a-z0-9._-]{2,63}$/i;
const TYPE = new Set(["arrival", "social", "golf", "farm", "creation", "learning", "pets", "explore"]);
function check(value, reason) { if (!value) throw new TypeError(reason); }

export function validateHubManifest(raw) {
  check(raw && typeof raw === "object" && !Array.isArray(raw) &&
    raw.schema === 1 && typeof raw.id === "string" && ID.test(raw.id), "Invalid hub manifest");
  check(Array.isArray(raw.areas) && raw.areas.length >= 3 && raw.areas.length <= 32, "Invalid hub areas");
  check(typeof raw.spawnAreaId === "string", "Missing spawn id");
  const ids = new Set();
  for (const area of raw.areas) {
    check(area && typeof area.id === "string" && ID.test(area.id) &&
      !ids.has(area.id), "Invalid/duplicate area id");
    ids.add(area.id);
    check(typeof area.name === "string" && area.name.length >= 3 && area.name.length <= 60,
      "Invalid area name");
    check(TYPE.has(area.kind), "Unknown area kind");
    check(Array.isArray(area.position) && area.position.length === 3 &&
      area.position.every((coordinate) => Number.isFinite(coordinate) &&
        Math.abs(coordinate) <= 500), "Invalid hub position");
    check(typeof area.watchIcon === "string" && ID.test(area.watchIcon),
      "Missing watch icon");
  }
  check(ids.has(raw.spawnAreaId), "Spawn area does not exist");
  check(Array.isArray(raw.paths) && raw.paths.length >= 1 &&
    raw.paths.length <= 64, "Invalid path definitions");
  const edges = new Set();
  const adjacency = new Map([...ids].map((id) => [id, new Set()]));
  for (const path of raw.paths) {
    check(Array.isArray(path) && path.length === 2 && ids.has(path[0]) &&
      ids.has(path[1]) && path[0] !== path[1], "Path references unknown/identical area");
    const key = [...path].sort().join(":");
    check(!edges.has(key), "Duplicate path");
    edges.add(key);
    adjacency.get(path[0]).add(path[1]);
    adjacency.get(path[1]).add(path[0]);
  }
  // Every map destination must be reachable from spawn on foot.
  const visited = new Set([raw.spawnAreaId]);
  const queue = [raw.spawnAreaId];
  for (const id of queue) {
    for (const next of adjacency.get(id)) if (!visited.has(next)) {
      visited.add(next); queue.push(next);
    }
  }
  check(visited.size === ids.size, "Disconnected public map area");
  check(raw.cinematic && raw.cinematic.skippable === true &&
    raw.cinematic.reducedMotionVariant === true &&
    Array.isArray(raw.cinematic.shots) && raw.cinematic.shots.length >= 2 &&
    raw.cinematic.shots.length <= 12 &&
    raw.cinematic.shots.every((shot) =>
      typeof shot.id === "string" && ID.test(shot.id) &&
      Number.isFinite(shot.seconds) && shot.seconds >= 0.5 &&
      shot.seconds <= 15 && typeof shot.description === "string"),
    "Hub cinematic must be bounded, skippable and comfortable");
  return raw;
}

export function getWatchHubMap(hub) {
  const clean = validateHubManifest(hub);
  return Object.freeze({
    sceneId: clean.id,
    spawnAreaId: clean.spawnAreaId,
    // Read-only buttons: destination teleport/pathfinding requires a trusted server check.
    buttons: clean.areas.map((area) => Object.freeze({
      id: area.id, label: area.name, icon: area.watchIcon,
      position: Object.freeze([...area.position]),
      intent: "navigate_to_area",
    })),
  });
}
