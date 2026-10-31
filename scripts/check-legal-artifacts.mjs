#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, openSync, readSync, closeSync, readdirSync, statSync } from "node:fs";
import { dirname, extname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const IGNORED_DIRS = new Set([
  ".git",
  "node_modules",
  "target",
  "dist",
  "coverage",
  ".cache",
  ".vite",
  "test-results",
  "playwright-report",
]);

const FORBIDDEN_FILENAME_PATTERNS = [
  /^scph[-_]?\d+.*\.(bin|rom|rom0|mec|nvm|img)$/i,
  /^ps2[-_]?bios.*\.(bin|rom|rom0|img)$/i,
  /_EXP\.inc$/i,
  /^FONTDATA\.inc$/i,
  /^MENUGEOM\.inc$/i,
  /\.(rom0|mec|nvm|p2s|idb|id0|id1|nam|til)$/i,
];

const ROMDIR_SIGNATURES = [
  Buffer.from("RESET\0\0\0\0\0", "ascii"),
  Buffer.from("TEXIMAGE\0\0", "ascii"),
  Buffer.from("SNDIMAGE\0\0", "ascii"),
  Buffer.from("FNTIMAGE\0\0", "ascii"),
];

/**
 * Scans a directory tree for Sony BIOS dumps, ROMDIR containers, or extracted `osdbits/res/*.inc` assets.
 * @param {string} rootDir
 * @returns {{ violations: Array<{ path: string; reason: string }> }}
 */
export function scanDirectoryForProprietaryArtifacts(rootDir) {
  /** @type {Array<{ path: string; reason: string }>} */
  const violations = [];

  /** @param {string} currentDir */
  function walk(currentDir) {
    for (const entry of readdirSync(currentDir, { withFileTypes: true })) {
      if (IGNORED_DIRS.has(entry.name)) continue;
      const fullPath = join(currentDir, entry.name);
      const relPath = relative(rootDir, fullPath).replace(/\\/g, "/");

      if (entry.isDirectory()) {
        if (relPath === "osdbits/res") {
          const children = readdirSync(fullPath);
          if (children.length > 0) {
            violations.push({
              path: relPath,
              reason: "Extracted BIOS resource directory osdbits/res/ must not exist in repository",
            });
          }
          continue;
        }
        walk(fullPath);
        continue;
      }

      if (!entry.isFile()) continue;

      for (const pat of FORBIDDEN_FILENAME_PATTERNS) {
        if (pat.test(entry.name)) {
          violations.push({
            path: relPath,
            reason: `Forbidden proprietary/BIOS filename pattern (${pat})`,
          });
        }
      }

      // Inspect binary header for PS2 ROMDIR / BIOS container signatures
      const st = statSync(fullPath);
      if (st.size >= 16) {
        const ext = extname(entry.name).toLowerCase();
        // Skip source/text docs that mention ROMDIR in prose
        const proseExts = new Set([".md", ".txt", ".c", ".h", ".py", ".rs", ".ts", ".mjs", ".js", ".json", ".vu", ".vsm", ".dsm", ".tsv", ".yml", ".yaml"]);
        if (!proseExts.has(ext)) {
          const buf = Buffer.alloc(32);
          const fd = openSync(fullPath, "r");
          let bytesRead = 0;
          try {
            bytesRead = readSync(fd, buf, 0, 32, 0);
          } finally {
            closeSync(fd);
          }
          const slice = buf.subarray(0, bytesRead);
          for (const sig of ROMDIR_SIGNATURES) {
            if (slice.subarray(0, sig.length).equals(sig)) {
              violations.push({
                path: relPath,
                reason: `Binary header matches PS2 BIOS ROMDIR container signature (${sig.toString("ascii").replace(/\0/g, "")})`,
              });
            }
          }
        }
      }
    }
  }

  if (existsSync(rootDir)) {
    walk(rootDir);
  }
  return { violations };
}

/**
 * Verifies that historical PS2 reconstruction files (`osdbits/`, `matching/`, `tools/`, `ps2expand.c`) remain untouched.
 * @param {string} repoRoot
 * @returns {{ ok: boolean; details: string }}
 */
export function verifyHistoricalProvenance(repoRoot) {
  const baseCommit = "ea0c68712b28b36fe033ed4fe488d86958de53d2";
  const checkCommit = spawnSync("git", ["cat-file", "-t", baseCommit], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (checkCommit.status !== 0) {
    // Shallow clone fallback: still verify core historical files exist
    const requiredFiles = [
      "osdbits/opening.c",
      "osdbits/menu.c",
      "osdbits/menuback.c",
      "osdbits/menuconfig.c",
      "osdbits/menutext.c",
      "osdbits/vucode_1.vsm",
      "ps2expand.c",
    ];
    for (const f of requiredFiles) {
      if (!existsSync(join(repoRoot, f))) {
        return { ok: false, details: `Missing historical source file: ${f}` };
      }
    }
    return { ok: true, details: "Shallow clone: verified historical files present" };
  }

  const diff = spawnSync(
    "git",
    [
      "diff",
      baseCommit,
      "--",
      "osdbits",
      "matching",
      "tools",
      "ps2expand.c",
      "vucode_1.vu",
      "vucode_2.vsm",
      "vucode_3.vsm",
      "vucode_4.vsm",
      "vucode_5.vsm",
      "vucode_6.vsm",
    ],
    { cwd: repoRoot, encoding: "utf8" },
  );
  if (diff.status !== 0) {
    return { ok: false, details: diff.stderr || "git diff failed" };
  }
  if ((diff.stdout ?? "").trim().length > 0) {
    return {
      ok: false,
      details: `Historical PS2 files were modified relative to ${baseCommit.slice(0, 7)}:\n${diff.stdout.slice(0, 800)}`,
    };
  }
  return { ok: true, details: "Historical PS2 files match ea0c687 byte-for-byte" };
}

const isMain =
  process.argv[1] &&
  resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url));

if (isMain) {
  const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const { violations } = scanDirectoryForProprietaryArtifacts(repoRoot);
  const provenance = verifyHistoricalProvenance(repoRoot);

  if (violations.length > 0) {
    console.error("ERROR: Proprietary / BIOS artifact check failed:");
    for (const v of violations) {
      console.error(`  - ${v.path}: ${v.reason}`);
    }
    process.exit(1);
  }
  if (!provenance.ok) {
    console.error(`ERROR: Historical provenance check failed: ${provenance.details}`);
    process.exit(1);
  }
  console.log(`Legal & provenance check passed (${provenance.details}; 0 forbidden artifacts).`);
}
