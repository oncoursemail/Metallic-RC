/**
 * Workshop and achievement contract prototype. Never treats AI tutoring,
 * attendance or self-reported client messages as accredited qualifications.
 * Authoritative caller identity must be verified by the service boundary.
 */
const ID = /^[a-z0-9][a-z0-9._-]{2,63}$/i;
export const EVENT_TO_ACHIEVEMENT = Object.freeze({
  hub_putt_scored: "hub.first-putt",
  invention_published: "creator.first-publish",
  workshop_assessed: "learning.first-workshop",
});
const EVENT_CALLERS = Object.freeze({
  hub_putt_scored: "room-server",
  invention_published: "publishing-service",
  workshop_assessed: "education-service",
});

export function validateWorkshop(workshop) {
  if (!workshop || typeof workshop !== "object" || Array.isArray(workshop) ||
      workshop.schema !== 1 || typeof workshop.id !== "string" || !ID.test(workshop.id) ||
      typeof workshop.title !== "string" || workshop.title.length < 5 || workshop.title.length > 120 ||
      !["beginner", "intermediate", "advanced"].includes(workshop.level) ||
      !Array.isArray(workshop.objectives) || workshop.objectives.length < 1 ||
      workshop.objectives.length > 12 ||
      !workshop.objectives.every((o) => typeof o === "string" && o.length >= 12 && o.length <= 200) ||
      !Array.isArray(workshop.rubric) || workshop.rubric.length < 1 ||
      workshop.rubric.length > 8 ||
      !workshop.rubric.every((r) => r && typeof r.criterion === "string" &&
        r.criterion.length >= 5 && r.criterion.length <= 100 &&
        Number.isSafeInteger(r.maxPoints) && r.maxPoints >= 1 && r.maxPoints <= 100)) {
    throw new TypeError("Invalid workshop definition");
  }
  return Object.freeze({ ...workshop });
}

/** Only a verified assessment with proof can unlock an achievement. */
export function applyVerifiedAchievement(awards, event, authority) {
  if (!Array.isArray(awards) || !awards.every((a) => typeof a === "string" && ID.test(a))) {
    throw new TypeError("Invalid existing award ids");
  }
  if (!event || typeof event !== "object" ||
      !Object.hasOwn(EVENT_TO_ACHIEVEMENT, event.kind) ||
      typeof event.accountId !== "string" || !ID.test(event.accountId) ||
      typeof event.evidenceId !== "string" || !ID.test(event.evidenceId) ||
      event.synthetic !== false) {
    throw new TypeError("Invalid or synthetic achievement event");
  }
  if (!authority || authority.authenticatedService !== true ||
      authority.role !== EVENT_CALLERS[event.kind] ||
      authority.accountId !== event.accountId) {
    throw new Error("Verified authoritative service required");
  }
  // Human-reviewed learning credit: an AI conversation by itself never qualifies.
  if (event.kind === "workshop_assessed" &&
      (event.instructorReviewed !== true || event.rubricPassed !== true)) {
    throw new Error("Human-reviewed rubric evidence required");
  }
  if (event.kind === "invention_published" && event.moderationApproved !== true) {
    throw new Error("Published invention must be approved");
  }
  const next = new Set(awards);
  const achievement = EVENT_TO_ACHIEVEMENT[event.kind];
  const awardedNow = !next.has(achievement);
  next.add(achievement);
  return Object.freeze({ awards: Object.freeze([...next].sort()), awardedNow,
    achievement, evidenceId: event.evidenceId });
}
