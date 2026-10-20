import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { validateInvention, searchInventions, INVENTION_CATEGORIES } from "../creator/catalog.mjs";
import { makeCompanion, stepCompanion } from "../ai/roomie.mjs";
import { validateWorkshop, applyVerifiedAchievement } from "../learning/education.mjs";
import { buildWatchMenu } from "../watch/menu.mjs";
import { canPerformTeachingAction } from "../learning/host-policy.mjs";

const load = (path) => JSON.parse(readFileSync(new URL(path, import.meta.url), "utf8"));
const sha256 = "a".repeat(64);
const sample = (overrides = {}) => ({
  schema: 1, id: "asset.sample", authorId: "author_001", title: "Windy Trees",
  description: "Original animated oak trees", category: "animations",
  rating: "everyone", status: "approved", revision: "v1",
  approvedRevision: "v1", approvedAt: "2026-10-18T00:00:00Z",
  licenseVerified: true, tags: ["forest", "nature"],
  files: [{ sha256, bytes: 1234 }], ...overrides,
});
const trusted = (accountId, role) => ({ authenticatedService: true, accountId, role });
const event = (kind, more = {}) => ({
  kind, accountId: "player_001", evidenceId: "evidence.001", synthetic: false, ...more,
});
test("public invention results exclude unapproved, stale revisions, mature and direct files", () => {
  const records = [
    sample(),
    sample({ id: "asset.review", status: "in_review", title: "Awaiting review" }),
    sample({ id: "asset.changed", revision: "v2", title: "Unapproved new revision" }),
    sample({ id: "asset.adult", title: "Age restricted", rating: "adult" }),
    sample({ id: "asset.teen", title: "Teen Garden", rating: "teen" }),
  ];
  const publicResult = searchInventions(records);
  assert.equal(publicResult.total, 1);
  assert.equal(publicResult.results[0].title, "Windy Trees");
  assert.equal(Object.hasOwn(publicResult.results[0], "files"), false);
  assert.equal(searchInventions(records, { maxRating: "teen" }).total, 2);
  assert.equal(searchInventions(records, { category: "animations", requiredTags: ["forest"], query: "oak" }).total, 1);
  assert.equal(INVENTION_CATEGORIES.includes("circuits"), true);
});
test("catalog rejects forged approvals, invalid files, duplicate IDs and arbitrary sorts", () => {
  assert.throws(() => validateInvention(sample({ licenseVerified: false })), /approval/);
  assert.throws(() => validateInvention(sample({ revision: "v2" })), /approval/);
  assert.throws(() => validateInvention(sample({ files: [{ sha256: "123", bytes: 12 }] })), /digest/);
  assert.throws(() => searchInventions([sample(), sample()]), /Duplicate/);
  assert.throws(() => searchInventions([sample()], { sort: "injected" }), /sort/);
});
test("Roomie reactions are deterministic, bounded, non-sentient gameplay cues", () => {
  const initial = makeCompanion({ id: "roomie.001", name: "Moss", voice: "warm" });
  const danger = stepCompanion(initial, "danger");
  assert.equal(danger.state.mood, "scared");
  assert.deepEqual(danger.effects, ["cower_animation"]);
  assert.equal(stepCompanion(danger.state, "follow").state.activity, "idle");
  const recovery = stepCompanion(danger.state, "safe");
  assert.equal(recovery.state.fear, 1);
  assert.equal(stepCompanion(initial, "follow", { canFollow: false }).effects.length, 0);
  assert.deepEqual(stepCompanion(initial, "danger", { aiEnabled: false }).effects, []);
  assert.throws(() => stepCompanion(initial, "buy_player_item"), /Unknown/);
  assert.throws(() => stepCompanion(initial, "tick", { maxTicks: 0 }), /budget/);
});
test("workshop specs validate structured rubrics and real learning objectives", () => {
  const definitions = load("../learning/workshops.v1.json");
  assert.equal(definitions.workshops.length, 4);
  assert.ok(definitions.workshops.every((w) => validateWorkshop(w)));
  assert.throws(() => validateWorkshop({ ...definitions.workshops[0], rubric: [] }), /Invalid/);
});
test("only correctly scoped trusted non-synthetic events grant achievements, idempotently", () => {
  const valid = event("hub_putt_scored");
  const once = applyVerifiedAchievement([], valid, trusted("player_001", "room-server"));
  assert.equal(once.awardedNow, true);
  const again = applyVerifiedAchievement(once.awards, valid, trusted("player_001", "room-server"));
  assert.equal(again.awardedNow, false);
  assert.throws(() => applyVerifiedAchievement([], valid, trusted("player_001", "education-service")), /service/);
  assert.throws(() => applyVerifiedAchievement([], { ...valid, synthetic: true }, trusted("player_001", "room-server")), /synthetic/);
  assert.throws(() => applyVerifiedAchievement([], event("workshop_assessed"), trusted("player_001", "education-service")), /rubric/);
  assert.throws(() => applyVerifiedAchievement([], event("invention_published"), trusted("player_001", "publishing-service")), /approved/);
  assert.equal(applyVerifiedAchievement([], event("workshop_assessed", { instructorReviewed: true, rubricPassed: true }),
    trusted("player_001", "education-service")).achievement, "learning.first-workshop");
});
test("watch quick slots are whitelist-validated and input-specific", () => {
  const vr = buildWatchMenu({ input: "vr-ray", quickSlots: ["map", "learning", "roomie"] });
  assert.equal(vr.schema, 1);
  assert.equal(vr.sections.find((s) => s.id === "support").needsServerAuthorization, true);
  assert.equal(buildWatchMenu({ input: "touch" }).minTouchTargetDp, 48);
  assert.throws(() => buildWatchMenu({ quickSlots: ["map", "map"] }), /unique/);
  assert.throws(() => buildWatchMenu({ quickSlots: ["admin.godmode"] }), /known/);
  assert.throws(() => buildWatchMenu({ input: "telepathy" }), /input/);
});

test("teacher and host grants are workshop-scoped and never imply moderation access", () => {
  const now = Date.parse("2026-10-18T12:00:00Z");
  const context = { workshopId: "workshop.001", accountId: "teacher.001",
    workshopApproved: true, safetyReviewPassed: true, rosterVerified: true,
    assessmentEvidenceReady: true };
  const host = [{ accountId: "teacher.001", workshopId: "workshop.001", role: "workshop_host",
    verifiedByHuman: true, safeguardingApproved: true, expiresAt: "2026-12-01T00:00:00Z" }];
  assert.equal(canPerformTeachingAction(host, "workshop.host", context, now), true);
  assert.equal(canPerformTeachingAction(host, "workshop.progress.assess", context, now), false);
  assert.equal(canPerformTeachingAction(host, "moderation.user.ban", context, now), false);
  assert.equal(canPerformTeachingAction(host, "workshop.host",
    { ...context, workshopId: "workshop.002" }, now), false);
  assert.equal(canPerformTeachingAction(host, "workshop.host",
    { ...context, safetyReviewPassed: false }, now), false);
  assert.equal(canPerformTeachingAction(host.map((g) => ({ ...g, revokedAt: "2026-10-18" })),
    "workshop.host", context, now), false);
  assert.equal(canPerformTeachingAction(host.map((g) => ({ ...g, expiresAt: "2026-01-01T00:00:00Z" })),
    "workshop.host", context, now), false);
  const teacher = [{ ...host[0], role: "verified_instructor" }];
  assert.equal(canPerformTeachingAction(teacher, "workshop.progress.assess", context, now), true);
  assert.equal(canPerformTeachingAction(teacher, "workshop.progress.assess",
    { ...context, assessmentEvidenceReady: false }, now), false);
});
