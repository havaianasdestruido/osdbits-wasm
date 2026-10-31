import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = process.cwd();

describe("Phase 11 — CI Workflow, Documentation Integrity & TODO.MD Completion", () => {
  it("validates .github/workflows/ci.yml references existing npm scripts and runs full verification stages", () => {
    const workflowPath = resolve(ROOT, ".github/workflows/ci.yml");
    expect(existsSync(workflowPath)).toBe(true);
    const yaml = readFileSync(workflowPath, "utf8");
    const pkg = JSON.parse(readFileSync(resolve(ROOT, "package.json"), "utf8")) as {
      scripts: Record<string, string>;
    };

    const matches = Array.from(yaml.matchAll(/npm run ([a-z0-9:-]+)/g)).map((m) => m[1]!);
    expect(matches.length).toBeGreaterThanOrEqual(10);
    for (const scriptName of matches) {
      expect(
        pkg.scripts[scriptName],
        `CI workflow references missing npm script: ${scriptName}`,
      ).toBeDefined();
    }

    for (const required of [
      "check:legal",
      "lint:rust",
      "test:rust",
      "build:wasm",
      "typecheck",
      "test:unit",
      "build",
      "test:e2e",
      "test:visual",
      "test:perf",
      "test:prod",
    ]) {
      expect(matches).toContain(required);
    }
  });

  it("verifies all documentation files, relative markdown links, and TODO.MD task completion", () => {
    const docFiles = [
      "README.md",
      "TODO.MD",
      "docs/phase0-baseline.md",
      "docs/wasm-architecture.md",
      "docs/fidelity-and-provenance.md",
      "docs/legal-and-assets.md",
      "docs/testing-and-deployment.md",
    ];

    for (const rel of docFiles) {
      const full = resolve(ROOT, rel);
      expect(existsSync(full), `Missing doc file: ${rel}`).toBe(true);
      const content = readFileSync(full, "utf8");
      expect(content.length).toBeGreaterThan(400);

      // Check local relative links `[label](target)`
      for (const m of content.matchAll(/\]\(([^)#\s]+)\)/g)) {
        const target = m[1]!;
        if (target.startsWith("http://") || target.startsWith("https://")) continue;
        const resolved = resolve(dirname(full), target);
        expect(existsSync(resolved), `Broken relative link in ${rel}: ${target}`).toBe(true);
      }
    }

    const todo = readFileSync(resolve(ROOT, "TODO.MD"), "utf8");
    const unchecked = Array.from(todo.matchAll(/^### \[ \] `(P\d+-\d+)`/gm)).map((m) => m[1]);
    const checked = Array.from(todo.matchAll(/^### \[x\] `(P\d+-\d+)`/gm)).map((m) => m[1]);
    expect(unchecked).toEqual([]);
    expect(checked.length).toBe(35);
  });
});
