#!/usr/bin/env node
/* Real-service acceptance. Start Metallic.RC.Platform with an isolated data directory.
   Run from any directory after installing tests/browser dependencies and Chromium:
     node tests/browser/verify_creator.mjs
   Optional: METALLIC_CREATOR_URL, METALLIC_PLAYWRIGHT_MODULE (file or directory),
   METALLIC_CHROMIUM (executable). No responses are mocked and no game is launched.
   The suite creates one uniquely named draft and leaves it in the isolated store.
*/
import assert from "node:assert/strict";
import { mkdir, stat, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const artifactDir = join(repoRoot, "artifacts");
const serviceUrl = new URL(process.env.METALLIC_CREATOR_URL || "http://127.0.0.1:5187/");
assert(serviceUrl.protocol === "http:" && ["127.0.0.1", "localhost", "[::1]"].includes(serviceUrl.hostname),
  "Acceptance tests only permit a loopback HTTP creator service.");
assert(serviceUrl.pathname === "/" && !serviceUrl.search && !serviceUrl.hash && !serviceUrl.username && !serviceUrl.password,
  "Use the creator service root URL without credentials, query, or fragment.");

let playwrightModule = "playwright";
if (process.env.METALLIC_PLAYWRIGHT_MODULE) {
  let location = resolve(process.env.METALLIC_PLAYWRIGHT_MODULE);
  if ((await stat(location)).isDirectory()) location = join(location, "index.mjs");
  playwrightModule = pathToFileURL(location).href;
}
const { chromium } = await import(playwrightModule);
await mkdir(artifactDir, { recursive: true });
const runId = new Date().toISOString().replace(/[:.]/g, "-");
const draftName = `Score counter · QA ${runId}`;
const report = { schemaVersion: 1, startedAt: new Date().toISOString(), service: serviceUrl.origin,
  scope: "Real local creator API and browser UI; no game-client or multiplayer test", checks: [], layouts: [],
  browserErrors: [], requestFailures: [], serverErrors: [], screenshots: [] };
const browser = await chromium.launch({ headless: true,
  ...(process.env.METALLIC_CHROMIUM ? { executablePath: resolve(process.env.METALLIC_CHROMIUM) } : {}) });
let page;

const exampleGraph = () => ({ schemaVersion: 1, entryNodeId: "start", nodes: [
  { id: "start", kind: "event.start", config: {} },
  { id: "first", kind: "constant.number", config: { value: 15 } },
  { id: "second", kind: "constant.number", config: { value: 27 } },
  { id: "sum", kind: "number.add", config: {} },
  { id: "output", kind: "debug.log", config: {} }
], edges: [
  { fromNode: "start", fromPort: "next", toNode: "output", toPort: "exec" },
  { fromNode: "first", fromPort: "value", toNode: "sum", toPort: "a" },
  { fromNode: "second", fromPort: "value", toNode: "sum", toPort: "b" },
  { fromNode: "sum", fromPort: "value", toNode: "output", toPort: "value" }
] });

function observe(browserPage, name) {
  browserPage.setDefaultTimeout(12000);
  browserPage.on("pageerror", error => report.browserErrors.push({ page: name, kind: "pageerror", message: error.message }));
  browserPage.on("console", message => {
    if (message.type() === "error") report.browserErrors.push({ page: name, kind: "console", message: message.text() });
  });
  browserPage.on("requestfailed", request => {
    const error = request.failure()?.errorText || "Unknown request failure";
    if (error !== "net::ERR_ABORTED") report.requestFailures.push({ page: name, path: new URL(request.url()).pathname, error });
  });
  browserPage.on("response", response => {
    if (response.status() >= 500) report.serverErrors.push({ page: name, path: new URL(response.url()).pathname, status: response.status() });
  });
  browserPage.on("dialog", dialog => dialog.accept());
}
async function check(name, fn) {
  const start = performance.now();
  try {
    await fn();
    report.checks.push({ name, passed: true, durationMs: Math.round(performance.now() - start) });
    console.log(`PASS ${name}`);
  } catch (error) {
    report.checks.push({ name, passed: false, message: error.message, durationMs: Math.round(performance.now() - start) });
    throw error;
  }
}
async function ready(browserPage) {
  await browserPage.waitForFunction(() => document.getElementById("service-state")?.textContent === "Creator service online"
    && !document.getElementById("save")?.disabled);
}
async function requestByClick(browserPage, button, method, path, status = 200) {
  const responsePromise = browserPage.waitForResponse(response => response.request().method() === method
    && new URL(response.url()).pathname === path);
  await browserPage.locator(button).click();
  const response = await responsePromise;
  assert.equal(response.status(), status, `${method} ${path} HTTP status`);
  const body = await response.json();
  await browserPage.waitForFunction(() => !document.getElementById("save").disabled);
  return { response, body };
}
async function useJson(browserPage, graph) {
  await browserPage.locator("#json-tab").click();
  await browserPage.locator("#graph-json").fill(JSON.stringify(graph, null, 2));
  await browserPage.locator("#visual-tab").click();
  assert(await browserPage.locator("#graph-view").isVisible(), "Valid graph source must return to Circuit view");
}
async function previewValue(browserPage, expected) {
  const { body } = await requestByClick(browserPage, "#preview", "POST", "/api/v1/cv3/preview");
  assert.equal(body.succeeded, true, JSON.stringify(body.issues));
  assert(body.logs.some(log => log.nodeId === "output" && log.text === String(expected)), "Preview must return the computed value");
  assert.match(await browserPage.locator("#output").innerText(), new RegExp(`output \\[number\\]\\s+${expected}(?:\\s|$)`));
  assert.equal(await browserPage.locator("#result-summary").textContent(), "Preview completed");
}
async function captureLayout(browserPage, screen) {
  const layout = await browserPage.evaluate(() => {
    const doc = document.documentElement;
    const selectors = ["main", ".topbar", ".editor-head", ".editor-layout", ".results", "footer", "#save", "#preview", "#validate"];
    const clipped = selectors.map(selector => {
      const rect = document.querySelector(selector).getBoundingClientRect();
      return { selector, x: rect.x, right: rect.right, width: rect.width };
    }).filter(box => box.width < 1 || box.x < -1 || box.right > doc.clientWidth + 1);
    const textMeasure = document.createElement("canvas").getContext("2d");
    const unreadableSelectors = Array.from(document.querySelectorAll(".connection-form select")).map(select => {
      const style = getComputedStyle(select);
      const label = select.selectedOptions[0]?.textContent || "";
      textMeasure.font = style.font;
      const textWidth = textMeasure.measureText(label).width;
      const availableWidth = select.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight) - 20;
      return { id: select.id, label, textWidth, availableWidth };
    }).filter(select => select.label && select.textWidth > select.availableWidth + 2);
    return { viewportWidth: doc.clientWidth, documentWidth: doc.scrollWidth, bodyWidth: document.body.scrollWidth,
      graphViewportWidth: document.getElementById("graph-view").clientWidth,
      graphContentWidth: document.getElementById("graph-view").scrollWidth, clipped, unreadableSelectors };
  });
  report.layouts.push({ screen, ...layout });
  const fileName = `creator-browser-${screen}.png`;
  await browserPage.screenshot({ path: join(artifactDir, fileName), fullPage: true, animations: "disabled" });
  report.screenshots.push(`artifacts/${fileName}`);
  assert(layout.documentWidth <= layout.viewportWidth + 1 && layout.bodyWidth <= layout.viewportWidth + 1,
    `Page has horizontal overflow: ${JSON.stringify(layout)}`);
  assert.equal(layout.clipped.length, 0, `Primary sections or actions are clipped: ${JSON.stringify(layout.clipped)}`);
  assert.equal(layout.unreadableSelectors.length, 0,
    `Connection selectors cannot display their selected labels: ${JSON.stringify(layout.unreadableSelectors)}`);
}

try {
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, deviceScaleFactor: 1, reducedMotion: "reduce" });
  page = await context.newPage();
  observe(page, "desktop");
  let roomId;
  await check("real service identity and initial circuit", async () => {
    const response = await page.goto(serviceUrl.href, { waitUntil: "networkidle" });
    assert.equal(response.status(), 200);
    await ready(page);
    const capabilitiesResponse = await page.request.get(new URL("/api/v1/capabilities", serviceUrl).href);
    assert.equal(capabilitiesResponse.status(), 200);
    const capabilities = await capabilitiesResponse.json();
    assert.equal(capabilities.product, "Metallic-RC");
    assert.equal(capabilities.apiVersion, "1");
    assert.equal(capabilities.mode, "local-authoring");
    assert.equal(capabilities.features.cv3Preview, true);
    assert.equal(capabilities.features.compiledClientAdapter, false);
    report.capabilities = capabilities;
    // Repeated runs may already contain this suite's saved draft. Use a fresh
    // unsaved example without deleting or overwriting any pre-existing draft.
    if ((await page.locator("#draft-state").innerText()).startsWith("Saved draft")) {
      await page.locator("#new-draft").click();
      await page.waitForFunction(() => !document.getElementById("save").disabled);
      await useJson(page, exampleGraph());
    }
    assert.equal(await page.locator("#graph-count").textContent(), "5 nodes · 4 connections");
  });
  await check("initial validation and actual preview result 42", async () => {
    const { body } = await requestByClick(page, "#validate", "POST", "/api/v1/cv3/validate");
    assert.equal(body.valid, true, JSON.stringify(body.issues));
    assert.equal(body.nodeCount, 5);
    assert.equal(body.edgeCount, 4);
    assert.equal(await page.locator("#result-summary").textContent(), "Validation passed");
    await previewValue(page, 42);
  });
  await check("create and save a new draft through the real API", async () => {
    await page.locator("#new-draft").click();
    await page.waitForFunction(() => !document.getElementById("save").disabled);
    await page.locator("#draft-name").fill(draftName);
    await page.locator("#draft-description").fill("Add two values and print the score.");
    await useJson(page, exampleGraph());
    const { response, body } = await requestByClick(page, "#save", "POST", "/api/v1/rooms", 201);
    roomId = body.id;
    assert.equal(body.name, draftName);
    assert.equal(body.revision, 1);
    assert.equal(response.headers().etag, '"r1"');
    assert.deepEqual(body.graph, exampleGraph());
    assert.equal(await page.locator("#draft-state").textContent(), "Saved draft · revision 1");
    report.createdDraft = { id: roomId, name: draftName };
  });
  await check("visual edit, revision save, and reload persistence", async () => {
    await page.locator('#graph .graph-node[aria-label^="first:"]').click();
    await page.getByLabel("first value", { exact: true }).fill("73");
    await page.locator("#draft-name").click();
    assert.equal(await page.locator("#draft-state").textContent(), "Draft · unsaved changes");
    const { response, body } = await requestByClick(page, "#save", "PUT", `/api/v1/rooms/${roomId}`);
    assert.equal(response.request().headers()["if-match"], '"r1"');
    assert.equal(body.revision, 2);
    assert.equal(body.graph.nodes.find(node => node.id === "first").config.value, 73);
    await page.reload({ waitUntil: "networkidle" });
    await ready(page);
    assert.equal(await page.locator("#draft-name").inputValue(), draftName);
    assert.equal(await page.locator("#draft-state").textContent(), "Saved draft · revision 2");
    await page.locator('#graph .graph-node[aria-label^="first:"]').click();
    assert.equal(await page.getByLabel("first value", { exact: true }).inputValue(), "73");
    await previewValue(page, 100);
  });
  await check("invalid JSON retention and valid return to Circuit", async () => {
    await page.locator("#json-tab").click();
    const validSource = await page.locator("#graph-json").inputValue();
    const invalidSource = '{"schemaVersion":1,"nodes":[';
    await page.locator("#graph-json").fill(invalidSource);
    await page.locator("#json-tab").click();
    assert.equal(await page.locator("#graph-json").inputValue(), invalidSource);
    await page.locator("#visual-tab").click();
    assert(await page.locator("#json-view").isVisible());
    assert.equal(await page.locator("#graph-json").inputValue(), invalidSource);
    assert.equal(await page.locator("#result-summary").textContent(), "Fix the JSON before switching views");
    assert(await page.locator("#output").evaluate(element => element.classList.contains("error")));
    assert(await page.locator("#add-node").isDisabled());
    assert(await page.getByLabel("first value", { exact: true }).isDisabled());
    await page.locator("#graph-json").fill(validSource);
    await page.locator("#visual-tab").click();
    assert(await page.locator("#graph-view").isVisible());
    assert.equal(await page.getByLabel("first value", { exact: true }).inputValue(), "73");
    const { body } = await requestByClick(page, "#save", "PUT", `/api/v1/rooms/${roomId}`);
    assert.equal(body.revision, 3);
    await previewValue(page, 100);
  });
  await check("named graph port wire endpoints", async () => {
    const ports = await page.evaluate(() => {
      const path = Array.from(document.querySelectorAll("#graph path")).find(element =>
        element.querySelector("title")?.textContent === "second.value → sum.b");
      const destination = document.querySelector('#graph .graph-node[aria-label^="sum:"]');
      const secondInput = destination.querySelectorAll('circle[cx="0"]')[1];
      const position = destination.transform.baseVal.consolidate().matrix;
      return { path: path?.getAttribute("d"), x: position.e + Number(secondInput.getAttribute("cx")),
        y: position.f + Number(secondInput.getAttribute("cy")) };
    });
    assert(ports.path.endsWith(`, ${ports.x} ${ports.y}`), `Wire does not reach the second input: ${JSON.stringify(ports)}`);
  });
  await check("desktop layout and screenshot", () => captureLayout(page, "desktop"));
  for (const screen of [{ name: "mobile", width: 390, height: 844 }, { name: "small-mobile", width: 320, height: 740 }]) {
    await check(`${screen.name} reload, preview, and layout`, async () => {
      const mobile = await browser.newContext({ viewport: { width: screen.width, height: screen.height },
        deviceScaleFactor: 1, isMobile: true, hasTouch: true, reducedMotion: "reduce" });
      try {
        const mobilePage = await mobile.newPage();
        observe(mobilePage, screen.name);
        await mobilePage.goto(serviceUrl.href, { waitUntil: "networkidle" });
        await ready(mobilePage);
        assert.equal(await mobilePage.locator("#draft-name").inputValue(), draftName);
        assert.equal(await mobilePage.locator("#draft-state").textContent(), "Saved draft · revision 3");
        await previewValue(mobilePage, 100);
        await captureLayout(mobilePage, screen.name);
      } finally { await mobile.close(); }
    });
  }
  await check("no unexpected browser, failed-request, or server errors", async () => {
    assert.deepEqual(report.browserErrors, []);
    assert.deepEqual(report.requestFailures, []);
    assert.deepEqual(report.serverErrors, []);
  });
  report.passed = true;
} catch (error) {
  report.passed = false;
  report.failure = error.message;
  console.error(`FAIL ${error.stack || error.message}`);
  if (page && !page.isClosed()) {
    await page.screenshot({ path: join(artifactDir, "creator-browser-failure.png"), fullPage: true, animations: "disabled" }).catch(() => {});
  }
  process.exitCode = 1;
} finally {
  await browser.close();
  report.finishedAt = new Date().toISOString();
  await writeFile(join(artifactDir, "creator-browser-results.json"), JSON.stringify(report, null, 2) + "\n");
  console.log(`${report.checks.filter(check => check.passed).length}/${report.checks.length} real-service browser checks passed.`);
}
