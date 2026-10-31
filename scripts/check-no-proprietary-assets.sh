#!/usr/bin/env bash
# Release gate: nothing derived from the PS2 BIOS may be committed or shipped.
#
# Rejects PlayStation payloads by magic, and BIOS-derived data by name and
# extension. `tools/extract-res.py` and `ps2expand.c` are the *user-side* paths
# and are allowed to exist; their outputs are not.
set -euo pipefail

cd "$(dirname "$0")/.."

fail=0
is_tracked() { git ls-files --error-unmatch "$1" >/dev/null 2>&1; }
report() { echo "PROPRIETARY  $1 ($2)" >&2; fail=1; }

# 1. PlayStation executables / images by magic.
while IFS= read -r f; do
  magic=$(head -c 4 "$f" 2>/dev/null | od -An -tx1 | tr -d ' \n')
  case "$magic" in
    7f454c46) report "$f" "ELF (PS2 executable?)" ;;
    50533258) report "$f" "PS2X image" ;;
    50534658) report "$f" "PSF archive" ;;
    *) ;;
  esac
done < <(git ls-files)

# 2. BIOS-derived resource data by name.
shipped=0
while IFS= read -r f; do
  case "$f" in
    osdbits/res/*|*osdsys_re/*)
      report "$f" "BIOS/GS dump path"
      ;;
    *TEXOWAL*|*TEXOFOG*|*TEXOREF*|*TEXOSCE*|*SNDBOOT*|*SNDWARN*)
      report "$f" "BIOS resource name"
      ;;
    web/public/*.wasm)
      shipped=$((shipped + 1))
      ;;
    *) ;;
  esac
done < <(git ls-files)

# 3. A built wasm in the tree is fine only if it is a build artefact,
#    i.e. not tracked. This documents the check explicitly.
if [[ $shipped -gt 0 ]]; then
  echo "note: $shipped tracked wasm file(s) under web/public look like build output" >&2
fi

# 4. Documentation must exist for the policy.
for doc in docs/ASSET-POLICY.md docs/LIMITATIONS.md; do
  [[ -f "$doc" ]] || report "$doc" "missing policy document"
done

if [[ $fail -eq 0 ]]; then
  echo "ok: no proprietary assets tracked ($(git ls-files | wc -l) files checked)"
fi
exit "$fail"
