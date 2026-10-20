/**
 * Workshop host entitlement policy. This is a pure contract, not authentication,
 * a verified teacher registry, nor a live scheduling/classroom service.
 *
 * Only a trusted backend may supply grants, verified profiles and rosters.
 * Teaching/hosting rights NEVER imply moderation, billing or admin rights.
 */
export const TEACHING_ACTIONS = Object.freeze([
  "workshop.schedule", "workshop.host", "workshop.materials.edit",
  "workshop.progress.assess", "workshop.cancel",
]);
const ACTIONS = new Set(TEACHING_ACTIONS);
const ROLE_ACTIONS = Object.freeze({
  workshop_host: new Set(["workshop.schedule", "workshop.host", "workshop.cancel"]),
  verified_mentor: new Set(["workshop.host", "workshop.materials.edit"]),
  verified_instructor: new Set(TEACHING_ACTIONS),
});
const ID = /^[a-z0-9][a-z0-9._-]{2,63}$/i;

export function canPerformTeachingAction(grants, action, context, now = Date.now()) {
  if (!Array.isArray(grants) || !ACTIONS.has(action) ||
      !context || typeof context.workshopId !== "string" || !ID.test(context.workshopId) ||
      typeof context.accountId !== "string" || !ID.test(context.accountId) ||
      !Number.isFinite(now)) return false;
  if (context.workshopApproved !== true || context.safetyReviewPassed !== true) return false;
  if (action === "workshop.progress.assess" &&
      (context.rosterVerified !== true || context.assessmentEvidenceReady !== true)) return false;
  return grants.some((grant) => {
    if (!grant || grant.accountId !== context.accountId || grant.revokedAt != null ||
        grant.workshopId !== context.workshopId ||
        !ROLE_ACTIONS[grant.role]?.has(action) ||
        grant.verifiedByHuman !== true || grant.safeguardingApproved !== true) return false;
    const expiry = typeof grant.expiresAt === "string" ? Date.parse(grant.expiresAt) : NaN;
    return Number.isFinite(expiry) && expiry > now;
  });
}
