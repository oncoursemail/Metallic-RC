/**
 * Metallic staff identity and privilege policy.
 * Pure functions for a trusted SERVER-SIDE role grant store.
 *
 * This module is NOT an authorization service or live database. It must receive
 * grants from an authenticated backend, never directly from a game client.
 * Staff visibility is cosmetic and NEVER grants permission.
 */
export const ROLES = Object.freeze({
  PLAYER: "player",
  ROOM_MOD: "room_moderator",
  VOLUNTEER_MOD: "volunteer_moderator",
  DEVELOPER: "developer",
  MODERATOR: "staff_moderator",
  LEAD_MOD: "lead_moderator",
  OPERATOR: "infrastructure_operator",
  OWNER: "owner",
});

export const TITLES = Object.freeze({
  VOLUNTEER_MOD: "volunteer_mod",
  DEVELOPER: "developer",
  MODERATOR: "moderator",
  LEAD_MOD: "lead_moderator",
  OPERATOR: "operations",
  OWNER: "metallic_staff",
});

const TITLE_TEXT = Object.freeze({
  [TITLES.VOLUNTEER_MOD]: "Volunteer Mod",
  [TITLES.DEVELOPER]: "Developer",
  [TITLES.MODERATOR]: "Moderator",
  [TITLES.LEAD_MOD]: "Lead Moderator",
  [TITLES.OPERATOR]: "Operations",
  [TITLES.OWNER]: "Metallic Staff",
});

const ROLE_TITLES = Object.freeze({
  [ROLES.VOLUNTEER_MOD]: TITLES.VOLUNTEER_MOD,
  [ROLES.DEVELOPER]: TITLES.DEVELOPER,
  [ROLES.MODERATOR]: TITLES.MODERATOR,
  [ROLES.LEAD_MOD]: TITLES.LEAD_MOD,
  [ROLES.OPERATOR]: TITLES.OPERATOR,
  [ROLES.OWNER]: TITLES.OWNER,
});

const CAPABILITIES = Object.freeze({
  [ROLES.ROOM_MOD]: new Set(["room.player.warn", "room.player.mute"]),
  [ROLES.VOLUNTEER_MOD]: new Set([
    "moderation.assigned_report.read",
    "moderation.player.warn",
    "moderation.room.mute",
    "moderation.case.escalate",
  ]),
  [ROLES.DEVELOPER]: new Set(["development.staging.inspect", "development.staging.test"]),
  [ROLES.MODERATOR]: new Set([
    "moderation.assigned_report.read",
    "moderation.player.warn",
    "moderation.room.mute",
    "moderation.case.escalate",
    "moderation.case.review",
  ]),
  [ROLES.LEAD_MOD]: new Set([
    "moderation.assigned_report.read",
    "moderation.player.warn",
    "moderation.room.mute",
    "moderation.case.escalate",
    "moderation.case.review",
    "moderation.role.propose",
  ]),
  [ROLES.OPERATOR]: new Set([
    "operations.fleet.read",
    "operations.staging.loadtest",
    "operations.room.drain",
  ]),
  [ROLES.OWNER]: new Set([
    "moderation.assigned_report.read",
    "moderation.player.warn",
    "moderation.room.mute",
    "moderation.case.escalate",
    "moderation.case.review",
    "moderation.role.propose",
    "moderation.role.grant",
    "moderation.role.revoke",
    "operations.fleet.read",
    "operations.staging.loadtest",
    "operations.room.drain",
    "development.staging.inspect",
  ]),
});

function normalizedGrants(grants, now) {
  if (!Array.isArray(grants)) return [];
  const checkedNow = now instanceof Date ? now.getTime() : Number(now);
  if (!Number.isFinite(checkedNow)) return [];
  return grants.filter((grant) => {
    if (!grant || typeof grant !== "object" || !Object.values(ROLES).includes(grant.role)) return false;
    if (grant.revokedAt != null) return false;
    const expiry = grant.expiresAt == null ? Infinity : Date.parse(grant.expiresAt);
    if (!(expiry > checkedNow)) return false;
    // A role without scope cannot be assumed to be global. The API must
    // distinguish project-wide grants from explicit room/test-region grants.
    if (grant.scopeType !== "global" && grant.scopeType !== "room" &&
        grant.scopeType !== "staging") return false;
    if (grant.scopeType !== "global" &&
        (typeof grant.scopeId !== "string" || grant.scopeId.length < 1)) return false;
    return true;
  });
}

/**
 * Resolve optional title from SERVER-verified grants and user preference.
 * title toggles do not change any underlying role or capability.
 */
export function resolvePublicStaffTitle(grants, preference, now = Date.now()) {
  const active = normalizedGrants(grants, now);
  const allowed = new Set(active.filter((grant) => grant.scopeType === "global")
    .map((grant) => ROLE_TITLES[grant.role]).filter(Boolean));
  const selection = preference && typeof preference.preferredTitle === "string"
    ? preference.preferredTitle : "";
  const eligible = allowed.has(selection);
  const title = eligible ? TITLE_TEXT[selection] : null;
  return {
    availableTitles: [...allowed].sort(),
    titleText: title,
    overhead: Boolean(title && preference?.showOverheadTitle === true),
    profile: Boolean(title && preference?.showProfileTitle === true),
    // Intentionally returns NO staff capability or trusted role token.
  };
}

/**
 * Internal permission predicate; backend still needs authentication, scope
 * checks, staff-action reason, MFA/step-up where required, and audit writing.
 */
export function canPerformStaffAction(grants, action, scope, now = Date.now()) {
  if (typeof action !== "string" || !scope || typeof scope !== "object") return false;
  if (scope.type !== "global" && scope.type !== "room" && scope.type !== "staging") return false;

  return normalizedGrants(grants, now).some((grant) => {
    if (!(CAPABILITIES[grant.role] || new Set()).has(action)) return false;

    // Ops/dev staging abilities never gain production scope accidentally.
    if (action.startsWith("development.staging.") || action === "operations.staging.loadtest") {
      return scope.type === "staging" &&
        (grant.scopeType === "global" ||
          (grant.scopeType === "staging" && grant.scopeId === scope.id));
    }

    // Production room-specific rules.
    if (action.startsWith("room.") || action === "moderation.room.mute") {
      return scope.type === "room" &&
        (grant.scopeType === "global" ||
         (grant.scopeType === "room" && grant.scopeId === scope.id));
    }

    // Global sanctions, staff changes and role proposals require global grant.
    return grant.scopeType === "global" && scope.type === "global";
  });
}

/**
 * Guard a requested title preference. Return ONLY validated preference fields.
 * Never accept arbitrary role or capabilities from request payload.
 */
export function validateTitlePreference(grants, patch, now = Date.now()) {
  if (!patch || typeof patch !== "object" || Array.isArray(patch)) {
    throw new TypeError("Expected a title preference object");
  }
  const fields = Object.keys(patch);
  const permitted = new Set(["preferredTitle", "showOverheadTitle", "showProfileTitle"]);
  if (fields.some((key) => !permitted.has(key))) {
    throw new TypeError("Unknown title setting; role grants cannot be edited here");
  }
  if (typeof patch.showOverheadTitle !== "boolean" ||
      typeof patch.showProfileTitle !== "boolean") {
    throw new TypeError("Title display settings must be explicit booleans");
  }
  const roleView = resolvePublicStaffTitle(grants, {
    preferredTitle: patch.preferredTitle,
    showOverheadTitle: patch.showOverheadTitle,
    showProfileTitle: patch.showProfileTitle,
  }, now);
  if (!roleView.titleText) throw new Error("Verified staff grant required for requested title");
  return Object.freeze({
    preferredTitle: patch.preferredTitle,
    showOverheadTitle: patch.showOverheadTitle,
    showProfileTitle: patch.showProfileTitle,
  });
}
