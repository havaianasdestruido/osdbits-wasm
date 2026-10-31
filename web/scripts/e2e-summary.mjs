#!/usr/bin/env node
// Turn the Playwright JSON report into GitHub Actions annotations.
//
// Why: the development sandbox has no browser binaries, so the E2E job is the
// only place these specs run. Job logs are not always reachable from there, but
// annotations are exposed through the checks API — this script makes a failure
// readable without downloading anything.
//
// Usage: node web/scripts/e2e-summary.mjs [path/to/report.json]
// Exit code is always 0: the test run itself already reported its status.

import { existsSync, readFileSync } from "node:fs";

const reportPath = process.argv[2] ?? "test-results/report.json";

const esc = (value) =>
  String(value)
    .replace(/%/g, "%25")
    .replace(/\r/g, "%0D")
    .replace(/\n/g, "%0A");

const firstLine = (message) => {
  const line = String(message ?? "")
    .split("\n")
    .map((s) => s.trim())
    .find((s) => s.length > 0);
  return (line ?? "failed without a message").slice(0, 400);
};

if (!existsSync(reportPath)) {
  console.log(`::warning title=Playwright::no JSON report at ${reportPath}`);
  process.exit(0);
}

let report;
try {
  report = JSON.parse(readFileSync(reportPath, "utf8"));
} catch (error) {
  console.log(`::warning title=Playwright::report is not valid JSON: ${esc(error.message)}`);
  process.exit(0);
}

const counts = { passed: 0, failed: 0, skipped: 0, timedOut: 0, interrupted: 0 };

const walk = (suite, trail) => {
  const path = suite.file ? [suite.file] : trail;
  for (const spec of suite.specs ?? []) {
    for (const test of spec.tests ?? []) {
      const project = test.projectName ?? "unknown";
      const outcome = test.outcome?.() ?? test.status;
      const results = test.results ?? [];
      const bad = results.find((r) => !["passed", "skipped"].includes(r.status));
      if (bad) {
        counts[bad.status] = (counts[bad.status] ?? 0) + 1;
        const file = spec.file ?? path[0] ?? "web/playwright";
        const title = spec.title ?? "(untitled)";
        console.log(
          `::error file=${file},title=Playwright ${esc(project)}::${esc(title)} — ${esc(
            firstLine(bad.error?.message ?? bad.status),
          )}`,
        );
      } else if (results.length > 0 && results.every((r) => r.status === "skipped")) {
        counts.skipped++;
      } else if (outcome === "skipped") {
        counts.skipped++;
      } else {
        counts.passed++;
      }
    }
  }
  for (const child of suite.suites ?? []) walk(child, path);
};

for (const suite of report.suites ?? []) walk(suite, []);

for (const error of report.errors ?? []) {
  console.log(`::error title=Playwright runner::${esc(firstLine(error.message ?? error))}`);
}

const summary = `${counts.passed} passed, ${counts.failed} failed, ${counts.timedOut} timed out, ${counts.skipped} skipped`;
console.log(`::notice title=Playwright::${esc(summary)}`);
console.log(summary);
