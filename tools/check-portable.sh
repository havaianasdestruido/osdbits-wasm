#!/usr/bin/env bash
# Run from any working directory; do not conceal intermediate command failures.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 -m ruff format --check tools/osd_resources.py tools/extract-res.py osdbits/tools/extract-res.py tests
python3 -m ruff check tools/osd_resources.py tools/extract-res.py osdbits/tools/extract-res.py tests
python3 -m compileall -q tools/osd_resources.py tools/extract-res.py osdbits/tools/extract-res.py tests
python3 -m unittest discover -s tests -v
git diff --check
