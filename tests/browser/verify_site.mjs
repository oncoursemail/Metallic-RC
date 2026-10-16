#!/usr/bin/env node
/* Metallic page geometry and real data screenshot QA, headless Chromium.
   Need to run against a LOCAL server exposing only the checked-out repo. */
import assert from "node:assert/strict";
import { mkdirSync, readFileSync } from "node:fs";
import { chromium } from "playwright";

const url = process.env.METALLIC_SITE_PREVIEW_URL || "http://127.0.0.1:8765/docs/site/";
const parsed = new URL(url);
if (parsed.protocol !== "http:" || !["127.0.0.1", "localhost", "[::1]", "::1"].includes(parsed.hostname)) {
  throw new Error("QA only permits a localhost static preview URL.");
}
const snapshot = JSON.parse(readFileSync("docs/progress.json", "utf8"));
const overall = snapshot.overall;
const complete = String(overall.closed);
const total = String(overall.total);
const percent = overall.total ? (100 * overall.closed / overall.total).toFixed(1) + "%" : "0.0%";
const desktopAndPhones = [
  { name: "tiny-phone", width: 320, height: 700, mobile: true },
  { name: "phone", width: 375, height: 812, mobile: true },
  { name: "tablet", width: 768, height: 1024, mobile: false },
  { name: "desktop", width: 1440, height: 900, mobile: false },
];
mkdirSync("artifacts/ui-screenshots", { recursive: true });
const errors = [];
const browser = await chromium.launch({ headless: true });
try {
  for (const screen of desktopAndPhones) {
    const context = await browser.newContext({
      viewport: { width: screen.width, height: screen.height },
      deviceScaleFactor: 1,
      isMobile: screen.mobile,
      hasTouch: screen.mobile,
      reducedMotion: "reduce",
    });
    const page = await context.newPage();
    const consoleErrors = [];
    page.on("pageerror", (e) => consoleErrors.push(String(e.message).slice(0, 160)));
    let response;
    try {
      response = await page.goto(url, { waitUntil: "networkidle", timeout: 25000 });
      assert(response && response.status() === 200, "site response must be 200");
      await page.waitForFunction(() => document.getElementById("progress-panel")?.getAttribute("aria-busy") === "false", {timeout: 8000});
      assert.equal((await page.locator("#count-closed").textContent())?.trim(), complete);
      assert.equal((await page.locator("#count-total").textContent())?.trim(), total);
      assert.equal((await page.locator("#overall-percent").textContent())?.trim(), percent);
      assert.equal(await page.locator("#main-meter").getAttribute("aria-valuenow"), percent.slice(0, -1));
      assert.equal(consoleErrors.length, 0, consoleErrors.join("; "));
      const layout = await page.evaluate(() => {
        const doc = document.documentElement;
        const key = ["#hero-title", "#roadmap-title", "#progress-panel", "#pillars-title", "#engineering-title", "#release-title"];
        const box = key.map((selector) => {
          const item = document.querySelector(selector);
          if (!item) return { selector, bad: true, error: "missing" };
          const r = item.getBoundingClientRect();
          return {
            selector,
            x: Number(r.x.toFixed(1)), y: Number(r.y.toFixed(1)),
            width: Number(r.width.toFixed(1)), height: Number(r.height.toFixed(1)),
            bad: !Number.isFinite(r.x) || r.width < 1 || r.x < -1 || (r.right > doc.clientWidth + 2)
          };
        });
        return {
          viewportWidth: doc.clientWidth,
          scrollWidth: doc.scrollWidth,
          bodyScrollWidth: document.body.scrollWidth,
          horizontalOverflow: doc.scrollWidth > doc.clientWidth + 2 || document.body.scrollWidth > doc.clientWidth + 2,
          visibilityFailures: box.filter(x => x.bad),
          genericButtons: Array.from(document.querySelectorAll("a[href]")).filter(x => x.getAttribute("href") === "#" || x.getAttribute("href") === "").length,
        };
      });
      assert(!layout.horizontalOverflow, "horizontal overflow " + JSON.stringify(layout));
      assert(!layout.visibilityFailures.length, "primary sections clipped " + JSON.stringify(layout));
      assert.equal(layout.genericButtons, 0, "placeholder links found");
      await page.screenshot({ path: "artifacts/ui-screenshots/" + screen.name + ".png", fullPage: true, animations: "disabled" });
      console.log("PASS " + screen.name + " " + screen.width + "px; exact GitHub count " + complete + "/" + total);
    } catch (error) {
      errors.push(screen.name + ": " + error.message);
      // Capture even broken layout so reviewers can see failures.
      await page.screenshot({ path: "artifacts/ui-screenshots/" + screen.name + "-failed.png", fullPage: true, timeout: 15000 }).catch(() => {});
      console.error("FAIL " + screen.name + ": " + error.message);
    } finally {
      await context.close();
    }
  }
} finally {
  await browser.close();
}
if (errors.length) {
  console.error("Metallic visual QA failed (" + errors.length + "):\n" + errors.join("\n"));
  process.exitCode = 1;
} else {
  console.log("All layout geometry, link and live progress source checks passed. Screenshots saved for human aesthetic review.");
}
