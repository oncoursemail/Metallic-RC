/**
 * Roomie and creator-owned pet prototype: deterministic, BOUNDED NPC
 * behavior. Mood/fear are gameplay variables, not actual sentience or emotion.
 * No language model, microphone, third-party voice clone or external service is
 * invoked here. Server authorization is required before actions are executed.
 */
export const MOODS = Object.freeze(["curious", "happy", "scared", "calm"]);
export const VOICES = Object.freeze(["silent", "warm", "bright", "low"]);
export const SIGNALS = Object.freeze([
  "greet", "play", "loud_noise", "danger", "safe", "leave", "follow", "wait", "tick",
]);
const SIGNAL_SET = new Set(SIGNALS);
const clamp = (value, min, max) => Math.max(min, Math.min(max, value));
const ID = /^[a-z0-9][a-z0-9._-]{2,63}$/i;

export function makeCompanion({ id, name, kind = "roomie", voice = "silent" }) {
  if (typeof id !== "string" || !ID.test(id)) throw new TypeError("Invalid companion id");
  if (typeof name !== "string" || name.trim() !== name || name.length < 1 || name.length > 48) {
    throw new TypeError("Invalid companion name");
  }
  if (!["roomie", "pet"].includes(kind) || !VOICES.includes(voice)) {
    throw new TypeError("Invalid kind or voice preset");
  }
  return Object.freeze({
    schema: 1, id, name, kind, voice, mood: "curious",
    fear: 0, bond: 0, activity: "idle", ticks: 0,
  });
}

/**
 * Signal values must come from validated game events on an authorized server,
 * never directly from untrusted player network packets or model instructions.
 * A companion has no store, filesystem, moderator, purchase or web actions.
 */
export function stepCompanion(state, signal, {
  aiEnabled = true, canFollow = true, maxTicks = 1000000,
} = {}) {
  if (!state || state.schema !== 1 || !MOODS.includes(state.mood) ||
      !VOICES.includes(state.voice) || !Number.isSafeInteger(state.ticks) ||
      !Number.isSafeInteger(state.fear) || state.fear < 0 || state.fear > 4 ||
      !Number.isSafeInteger(state.bond) || state.bond < 0 || state.bond > 10) {
    throw new TypeError("Invalid companion state");
  }
  if (!SIGNAL_SET.has(signal)) throw new TypeError("Unknown companion signal");
  if (!Number.isSafeInteger(maxTicks) || maxTicks < 1) throw new TypeError("Invalid budget");
  if (state.ticks >= maxTicks) throw new RangeError("Companion simulation budget exceeded");
  let fear = state.fear;
  let bond = state.bond;
  let activity = "idle";
  const effects = [];
  if (aiEnabled) {
    switch (signal) {
      case "danger": fear = clamp(fear + 3, 0, 4); effects.push("cower_animation"); break;
      case "loud_noise": fear = clamp(fear + 2, 0, 4); effects.push("startle_animation"); break;
      case "safe": fear = clamp(fear - 2, 0, 4); effects.push("look_around"); break;
      case "tick": fear = clamp(fear - 1, 0, 4); break;
      case "greet": bond = clamp(bond + 1, 0, 10); effects.push("wave_animation"); break;
      case "play": bond = clamp(bond + 2, 0, 10); effects.push("play_animation"); break;
      case "follow": if (canFollow && fear < 3) {
        activity = "following"; effects.push("pathfind_to_owner");
      } break;
      case "wait": activity = "waiting"; effects.push("sit_animation"); break;
      case "leave": activity = "idle"; break;
    }
  }
  const mood = !aiEnabled ? "calm" : fear >= 3 ? "scared" :
    ["greet", "play"].includes(signal) ? "happy" :
    fear > 0 ? "curious" : signal === "safe" || signal === "tick" ? "calm" : state.mood;
  // No network RPC, text response, or privileged command can be emitted.
  return Object.freeze({
    state: Object.freeze({ ...state, fear, bond, mood, activity, ticks: state.ticks + 1 }),
    effects: Object.freeze(aiEnabled ? effects : []),
  });
}
