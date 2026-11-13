/**
 * Support case policy and metadata projection. No persistence or API endpoints.
 * All principals, roles, case IDs, timestamps and assignments MUST be fetched
 * and verified by a trusted API/database layer; never trust client supplied
 * actor objects to call these functions directly.
 */
export const SUPPORT_CATEGORIES = Object.freeze([
  "technical", "account", "creator", "learning", "accessibility", "safety",
  "payments",
]);
export const SUPPORT_STATUSES = Object.freeze([
  "open", "waiting_for_player", "waiting_for_staff", "resolved", "closed",
]);
const CATEGORIES = new Set(SUPPORT_CATEGORIES);
const STATUSES = new Set(SUPPORT_STATUSES);
const ID = /^[a-z0-9][a-z0-9._-]{2,63}$/i;
const validId = (id) => typeof id === "string" && ID.test(id);
const isObject = (raw) => raw && typeof raw === "object" && !Array.isArray(raw);
function guard(condition, message) { if (!condition) throw new TypeError(message); }

export function createCaseRecord({
  caseId, ownerAccountId, category, subject, message, createdAt, channel = "web",
}, trustedContext) {
  guard(trustedContext?.authenticated === true &&
    trustedContext.accountId === ownerAccountId &&
    trustedContext.serverIssuedCaseId === caseId,
  "Authenticated server-issued case ownership required");
  guard(validId(caseId) && validId(ownerAccountId), "Invalid case/account id");
  guard(CATEGORIES.has(category), "Unknown support category");
  guard(typeof subject === "string" && subject.trim() === subject &&
    subject.length >= 5 && subject.length <= 120, "Subject must be 5-120 chars");
  guard(typeof message === "string" && message.trim() === message &&
    message.length >= 20 && message.length <= 3000, "Message must be 20-3000 chars");
  guard(["web", "watch", "launcher"].includes(channel), "Invalid intake channel");
  guard(typeof createdAt === "string" && Number.isFinite(Date.parse(createdAt)),
    "Trusted timestamp required");
  return Object.freeze({
    schema: 1, caseId, ownerAccountId, category, subject,
    message, status: "open", createdAt, updatedAt: createdAt, channel,
    assignedStaffAccountId: null, revision: 0,
  });
}
export function viewCaseRecord(record, principal) {
  guard(isObject(record) && record.schema === 1 && validId(record.caseId) &&
    validId(record.ownerAccountId) && STATUSES.has(record.status), "Invalid support case");
  guard(principal?.authenticated === true && validId(principal.accountId),
    "Authenticated user required");
  const isOwner = principal.accountId === record.ownerAccountId;
  const assignedAgent = principal.role === "support_agent" &&
    principal.accountId === record.assignedStaffAccountId;
  const safetyReviewer = principal.role === "safety_reviewer" &&
    record.category === "safety" && principal.assignedCases?.includes(record.caseId) === true;
  if (!isOwner && !assignedAgent && !safetyReviewer) throw new Error("Case access denied");
  // No internal staff notes or unrelated private user fields are returned.
  return Object.freeze({
    caseId: record.caseId, category: record.category, subject: record.subject,
    message: record.message, status: record.status,
    createdAt: record.createdAt, updatedAt: record.updatedAt, channel: record.channel,
  });
}
export function transitionCase(record, action, principal, nowIso) {
  guard(isObject(record) && record.schema === 1 && STATUSES.has(record.status) &&
    Number.isSafeInteger(record.revision) && record.revision >= 0, "Invalid support case state");
  guard(principal?.authenticated === true && validId(principal.accountId), "Authenticated actor required");
  guard(typeof nowIso === "string" && Number.isFinite(Date.parse(nowIso)) &&
    Date.parse(nowIso) >= Date.parse(record.updatedAt), "Timestamp must not go backward");
  guard(typeof action === "string", "Action required");
  const isOwner = principal.accountId === record.ownerAccountId;
  const staff = principal.role === "support_agent" &&
    principal.accountId === record.assignedStaffAccountId;
  const reviewer = principal.role === "safety_reviewer" &&
    record.category === "safety" && principal.assignedCases?.includes(record.caseId) === true;
  const canAct = staff || reviewer;
  const allowed = {
    open: { staff_wait: ["waiting_for_player", canAct],
      close: ["closed", canAct] },
    waiting_for_player: { player_reply: ["waiting_for_staff", isOwner],
      close: ["closed", canAct] },
    waiting_for_staff: { staff_resolve: ["resolved", canAct],
      staff_wait: ["waiting_for_player", canAct],
      close: ["closed", canAct] },
    resolved: { reopen: ["waiting_for_staff", isOwner], close: ["closed", canAct] },
    closed: {},
  };
  const entry = allowed[record.status]?.[action];
  if (!entry || !entry[1]) throw new Error("Unauthorized or invalid case transition");
  return Object.freeze({
    ...record, status: entry[0], updatedAt: nowIso, revision: record.revision + 1,
  });
}
