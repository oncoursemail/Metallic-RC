import test from "node:test";
import assert from "node:assert/strict";
import { createCaseRecord, viewCaseRecord, transitionCase } from "../case-policy.mjs";
const t0 = "2026-10-18T15:00:00Z";
const t1 = "2026-10-18T15:01:00Z";
const t2 = "2026-10-18T15:02:00Z";
const t3 = "2026-10-18T15:03:00Z";
const owner = { authenticated: true, accountId: "player.001", role: "player" };
const stranger = { authenticated: true, accountId: "player.002", role: "player" };
const staff = { authenticated: true, accountId: "agent.001", role: "support_agent" };
const safety = { authenticated: true, accountId: "safety.001", role: "safety_reviewer",
  assignedCases: ["case.001"] };
const sample = (category = "technical") => createCaseRecord({
  caseId: "case.001", ownerAccountId: "player.001", category,
  subject: "I cannot join my room",
  message: "Joining my room fails immediately after logging in.",
  createdAt: t0, channel: "web",
}, { ...owner, serverIssuedCaseId: "case.001" });
test("case intake requires trusted ownership and a server-issued case ID", () => {
  const record = sample();
  assert.equal(record.ownerAccountId, "player.001");
  assert.equal(record.status, "open");
  assert.throws(() => createCaseRecord({
    caseId: "case.001", ownerAccountId: "player.001", category: "technical",
    subject: "Need help", message: "A valid message of enough characters",
    createdAt: t0,
  }, { ...stranger, serverIssuedCaseId: "case.001" }), /ownership/);
  assert.throws(() => createCaseRecord({
    caseId: "case.001", ownerAccountId: "player.001", category: "technical",
    subject: "Need help", message: "short", createdAt: t0,
  }, { ...owner, serverIssuedCaseId: "case.001" }), /Message/);
});
test("support case privacy denies unrelated players and unrelated staff", () => {
  const record = sample();
  assert.equal(viewCaseRecord(record, owner).caseId, "case.001");
  assert.throws(() => viewCaseRecord(record, stranger), /denied/);
  assert.throws(() => viewCaseRecord(record, staff), /denied/);
  assert.throws(() => viewCaseRecord(record, safety), /denied/);
  const assigned = { ...record, assignedStaffAccountId: "agent.001" };
  assert.equal(viewCaseRecord(assigned, staff).caseId, "case.001");
  const alert = sample("safety");
  assert.equal(viewCaseRecord(alert, safety).category, "safety");
  assert.equal(Object.hasOwn(viewCaseRecord(alert, safety), "assignedStaffAccountId"), false);
});
test("only authorized actors perform state transitions and closed cases are terminal", () => {
  const assigned = { ...sample(), assignedStaffAccountId: "agent.001" };
  assert.throws(() => transitionCase(assigned, "staff_wait", stranger, t1), /Unauthorized/);
  assert.throws(() => transitionCase(assigned, "staff_resolve", staff, t1), /Unauthorized/);
  const waiting = transitionCase(assigned, "staff_wait", staff, t1);
  assert.equal(waiting.status, "waiting_for_player");
  assert.equal(waiting.revision, 1);
  assert.throws(() => transitionCase(waiting, "player_reply", staff, t2), /Unauthorized/);
  const replied = transitionCase(waiting, "player_reply", owner, t2);
  assert.equal(replied.status, "waiting_for_staff");
  const resolved = transitionCase(replied, "staff_resolve", staff, t3);
  assert.equal(resolved.status, "resolved");
  const closed = transitionCase(resolved, "close", staff, "2026-10-18T15:04:00Z");
  assert.equal(closed.status, "closed");
  assert.throws(() => transitionCase(closed, "reopen", owner, "2026-10-18T15:05:00Z"), /Unauthorized/);
  assert.throws(() => transitionCase(assigned, "staff_wait", staff, t0.replace("15:00", "14:59")), /Timestamp/);
});
