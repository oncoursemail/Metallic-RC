/**
 * Cross-input watch menu contract. Navigation intents are NOT authorization.
 * Users may customize quick slots from a fixed allowlist. Server permissions
 * for creator editing, transactions and staff actions are always checked later.
 */
export const WATCH_DESTINATIONS = Object.freeze([
  "map", "social", "avatar", "create", "inventions", "learning",
  "roomie", "pets", "achievements", "support", "settings",
]);
const DESTINATIONS = new Set(WATCH_DESTINATIONS);
export const WATCH_INPUTS = Object.freeze(["mouse", "gamepad", "vr-ray", "vr-direct", "touch"]);
export function buildWatchMenu({ quickSlots = ["map", "social", "create", "roomie"], input = "mouse" } = {}) {
  if (!Array.isArray(quickSlots) || quickSlots.length < 1 || quickSlots.length > 6 ||
      quickSlots.some((name) => !DESTINATIONS.has(name)) ||
      new Set(quickSlots).size !== quickSlots.length) {
    throw new TypeError("Quick slots must be unique known destinations");
  }
  if (!WATCH_INPUTS.includes(input)) throw new TypeError("Unknown input method");
  const sections = WATCH_DESTINATIONS.map((id) => Object.freeze({
    id, visible: true, intent: "open_screen", needsServerAuthorization:
      ["create", "inventions", "support"].includes(id),
  }));
  return Object.freeze({ schema: 1, input, quickSlots: Object.freeze([...quickSlots]),
    sections: Object.freeze(sections),
    reducedMotionSupported: true, minTouchTargetDp: input === "touch" ? 48 : null });
}
