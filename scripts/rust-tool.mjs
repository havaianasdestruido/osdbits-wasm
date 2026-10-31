#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, linkSync, mkdirSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sysrootDir = join(repoRoot, ".cache", "rust-toolchain");
const markerFile = join(sysrootDir, ".installed-1.88.0");

function linkTree(srcDir, dstDir) {
  mkdirSync(dstDir, { recursive: true });
  for (const entry of readdirSync(srcDir, { withFileTypes: true })) {
    if (entry.name === "manifest.in") continue;
    const srcPath = join(srcDir, entry.name);
    const dstPath = join(dstDir, entry.name);
    if (entry.isDirectory()) {
      linkTree(srcPath, dstPath);
    } else if (entry.isFile() || entry.isSymbolicLink()) {
      if (existsSync(dstPath)) continue;
      try {
        linkSync(srcPath, dstPath);
      } catch {
        copyFileSync(srcPath, dstPath);
      }
    }
  }
}

function ensureLocalRustSysroot() {
  const rustbinRoot = join(repoRoot, "node_modules", "@rustbin");
  const rustcComp = join(rustbinRoot, "rustc-1.88.0-x86_64-unknown-linux-gnu", "rustc");
  if (process.platform !== "linux" || process.arch !== "x64" || !existsSync(rustcComp)) {
    return null;
  }
  if (!existsSync(markerFile) || !existsSync(join(sysrootDir, "bin", "rustc"))) {
    const components = [
      rustcComp,
      join(rustbinRoot, "cargo-1.88.0-x86_64-unknown-linux-gnu", "cargo"),
      join(rustbinRoot, "rust-std-1.88.0-x86_64-unknown-linux-gnu", "rust-std-x86_64-unknown-linux-gnu"),
      join(rustbinRoot, "rust-std-1.88.0-wasm32-unknown-unknown", "rust-std-wasm32-unknown-unknown"),
      join(rustbinRoot, "rustfmt-1.88.0-x86_64-unknown-linux-gnu", "rustfmt-preview"),
      join(rustbinRoot, "clippy-1.88.0-x86_64-unknown-linux-gnu", "clippy-preview"),
    ];
    for (const comp of components) {
      if (!existsSync(comp) || !statSync(comp).isDirectory()) {
        throw new Error(`Missing @rustbin component directory: ${comp}`);
      }
      linkTree(comp, sysrootDir);
    }
    writeFileSync(markerFile, "1.88.0\n", "utf8");
  }
  return sysrootDir;
}

function buildRustEnv() {
  const sysroot = ensureLocalRustSysroot();
  if (!sysroot) {
    return { cargoBin: "cargo", env: { ...process.env } };
  }
  const binDir = join(sysroot, "bin");
  const libDir = join(sysroot, "lib");
  return {
    cargoBin: join(binDir, "cargo"),
    env: {
      ...process.env,
      RUSTC: join(binDir, "rustc"),
      RUSTDOC: join(binDir, "rustdoc"),
      PATH: `${binDir}:${process.env.PATH ?? ""}`,
      LD_LIBRARY_PATH: `${libDir}:${process.env.LD_LIBRARY_PATH ?? ""}`,
    },
  };
}

function runCargo(args) {
  const { cargoBin, env } = buildRustEnv();
  const res = spawnSync(cargoBin, args, {
    cwd: repoRoot,
    env,
    stdio: "inherit",
  });
  if (res.error) {
    throw res.error;
  }
  if (res.status !== 0) {
    process.exit(res.status ?? 1);
  }
}

const cmd = process.argv[2] ?? "build-wasm";
const extraArgs = process.argv.slice(3);

switch (cmd) {
  case "build-wasm": {
    runCargo([
      "build",
      "-p",
      "osdbits-wasm",
      "--target",
      "wasm32-unknown-unknown",
      "--release",
      ...extraArgs,
    ]);
    const wasmSrc = join(repoRoot, "target", "wasm32-unknown-unknown", "release", "osdbits_wasm.wasm");
    const publicWasmDir = join(repoRoot, "web", "public", "wasm");
    const generatedDir = join(repoRoot, "web", "src", "wasm", "generated");
    mkdirSync(publicWasmDir, { recursive: true });
    mkdirSync(generatedDir, { recursive: true });
    copyFileSync(wasmSrc, join(publicWasmDir, "osdbits_wasm.wasm"));
    copyFileSync(wasmSrc, join(generatedDir, "osdbits_wasm.wasm"));
    break;
  }
  case "test": {
    runCargo(["test", "--workspace", ...extraArgs]);
    break;
  }
  case "lint": {
    runCargo(["fmt", "--all", "--", "--check"]);
    runCargo(["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]);
    runCargo(["clippy", "-p", "osdbits-wasm", "--target", "wasm32-unknown-unknown", "--", "-D", "warnings"]);
    break;
  }
  case "fmt": {
    runCargo(["fmt", "--all", ...extraArgs]);
    break;
  }
  case "cargo": {
    runCargo(extraArgs);
    break;
  }
  default:
    console.error(`Unknown rust-tool subcommand: ${cmd}`);
    process.exit(2);
}
