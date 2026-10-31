#!/usr/bin/env bash
# The original PS2 implementation must stay byte-identical: this project adds
# to the repository, it does not replace it.
#
# The baseline hashes are the upstream commit recorded in UPSTREAM.md; when
# they are unavailable (shallow clone, no git) the script falls back to
# checking that the files still exist and that no upstream file was deleted.
set -euo pipefail

cd "$(dirname "$0")/.."

UPSTREAM_COMMIT="${UPSTREAM_COMMIT:-ea0c68712b28b36fe033ed4fe488d86958de53d2}"

paths=(
  osdbits/main.c osdbits/inc.h osdbits/opening.c osdbits/menu.c osdbits/menuback.c
  osdbits/menuconfig.c osdbits/menutext.c osdbits/pad.c osdbits/res.c osdbits/res.h
  osdbits/sound.c osdbits/Makefile osdbits/towerchain.dsm
  vucode_1.vu vucode_2.vu vucode_3.vu vucode_4.vu vucode_5.vu vucode_6.vu vucode_7.vu
  ps2expand.c tools/extract-res.py notes.txt
)

fail=0
for p in "${paths[@]}"; do
  if [[ ! -f "$p" ]]; then
    echo "MISSING  $p" >&2
    fail=1
  fi
done

if git rev-parse --git-dir >/dev/null 2>&1 && git cat-file -e "${UPSTREAM_COMMIT}^{commit}" 2>/dev/null; then
  changed=$(git diff --name-only "$UPSTREAM_COMMIT" -- "${paths[@]}" || true)
  if [[ -n "$changed" ]]; then
    echo "CHANGED upstream files since $UPSTREAM_COMMIT:" >&2
    echo "$changed" >&2
    fail=1
  else
    echo "ok: ${#paths[@]} upstream files are byte-identical to $UPSTREAM_COMMIT"
  fi
else
  echo "ok: ${#paths[@]} upstream files present (upstream commit unavailable, hash check skipped)"
fi

exit "$fail"
