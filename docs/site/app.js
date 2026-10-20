"use strict";

/**
 * Metallic-RC public development view.
 * Only generated aggregate GitHub issue completion; no fabricated player counts.
 */
(() => {
  const el = (id) => document.getElementById(id);
  const phaseRoot = el("phases");
  const panel = el("progress-panel");
  const PHASES = ["P0", "P1", "P2", "P3", "P4", "P5", "P6"];

  function whole(value) {
    const number = Number(value);
    return Number.isSafeInteger(number) && number >= 0 ? number : 0;
  }

  function getProgress(phase) {
    const total = whole(phase && phase.total);
    const done = Math.min(total, whole(phase && phase.closed));
    return { total, done, percent: total ? (100 * done) / total : 0 };
  }

  function renderPhase(key, source) {
    const info = getProgress(source);
    const phase = document.createElement("div");
    phase.className = "phase";
    const header = document.createElement("div");
    header.className = "phase-top";
    const title = document.createElement("div");
    title.className = "phase-title";
    const id = document.createElement("span");
    id.className = "phase-id";
    id.textContent = key;
    const name = document.createElement("span");
    name.className = "phase-name";
    name.textContent = typeof source.name === "string" ? source.name.slice(0, 65) : "Unspecified phase";
    const count = document.createElement("span");
    count.className = "phase-count";
    count.textContent = String(info.done) + " / " + String(info.total);
    title.append(id, name);
    header.append(title, count);
    const track = document.createElement("div");
    track.className = "phase-track";
    track.setAttribute("role", "progressbar");
    track.setAttribute("aria-label", key + ": " + name.textContent);
    track.setAttribute("aria-valuemin", "0");
    track.setAttribute("aria-valuemax", "100");
    track.setAttribute("aria-valuenow", info.percent.toFixed(1));
    const fill = document.createElement("span");
    fill.style.width = String(info.percent) + "%";
    track.append(fill);
    phase.append(header, track);
    return phase;
  }

  function showUnavailable(reason) {
    el("progress-note").textContent = reason;
    el("progress-timestamp").textContent = "";
    phaseRoot.replaceChildren();
    const note = document.createElement("p");
    note.className = "load-note";
    note.textContent = "Live issue totals are unavailable here. Use the GitHub tracker for the authoritative status.";
    phaseRoot.append(note);
    panel.setAttribute("aria-busy", "false");
  }

  function showSnapshot(data) {
    if (!data || data.schema_version !== 1 || !data.overall || !data.phases) {
      throw new Error("Invalid aggregate issue snapshot");
    }
    const all = getProgress(data.overall);
    el("count-closed").textContent = String(all.done);
    el("count-total").textContent = String(all.total);
    el("overall-percent").textContent = all.percent.toFixed(1) + "%";
    el("main-meter").setAttribute("aria-valuenow", all.percent.toFixed(1));
    el("main-fill").style.width = String(all.percent) + "%";
    el("progress-note").textContent = String(all.total - all.done) + " scoped issues open. Counts exclude pull requests and do not imply a playable game.";

    let timestamp = "Snapshot time unknown";
    if (typeof data.last_issue_activity_utc === "string") {
      const ms = Date.parse(data.last_issue_activity_utc);
      if (Number.isFinite(ms)) {
        timestamp = "Issue activity · " + new Date(ms).toLocaleDateString(undefined, {
          year: "numeric", month: "short", day: "numeric"
        });
      }
    }
    el("progress-timestamp").textContent = timestamp;
    phaseRoot.replaceChildren(...PHASES.map((key) => renderPhase(key, data.phases[key] || {})));
    panel.setAttribute("aria-busy", "false");
  }

  async function load() {
    try {
      const response = await fetch("../progress.json", { cache: "no-store", credentials: "omit" });
      if (!response.ok) throw new Error("HTTP " + response.status);
      showSnapshot(await response.json());
    } catch (_error) {
      showUnavailable("Could not load the current issue snapshot.");
    }
  }

  load();
})();
