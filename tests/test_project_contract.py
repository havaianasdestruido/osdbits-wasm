"""Executable checks for the roadmap and portable CI configuration."""

import re
import tomllib
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]


class ProjectContractTests(unittest.TestCase):
    def test_all_phases_and_unique_actionable_tasks(self):
        text = (ROOT / "TODO.MD").read_text()
        phases = re.findall(r"^## Phase (\d+) —", text, re.MULTILINE)
        self.assertEqual(phases, [str(i) for i in range(12)])
        tasks = re.findall(
            r"^- \[([ x!])\] `(P\d{2}-\d{3})` — ([^\n]+)\n(.*?)(?=^- \[|^## |\Z)",
            text,
            re.MULTILINE | re.DOTALL,
        )
        self.assertGreaterEqual(len(tasks), 45)
        ids = [task[1] for task in tasks]
        self.assertEqual(len(ids), len(set(ids)))
        for status, identifier, objective, detail in tasks:
            with self.subTest(task=identifier):
                self.assertTrue(objective.endswith("."))
                for field in ["Acceptance", "Tests", "Dependencies"]:
                    self.assertRegex(detail, rf"  - {field}: .+\.")
                if status == "!":
                    self.assertIn("  - Blocker:", detail)
                dependencies = re.search(r"  - Dependencies: (.*)", detail).group(1)
                for dependency in re.findall(r"P\d{2}-\d{3}", dependencies):
                    self.assertIn(dependency, ids)
                    self.assertNotEqual(dependency, identifier)
        self.assertIn("Not release-ready", text)
        self.assertIn("## Deferred features and known limitations", text)

    def test_portable_workflow_runs_the_local_gate_without_private_assets(self):
        # BaseLoader avoids YAML 1.1's surprising boolean treatment of `on`.
        workflow = yaml.load(
            (ROOT / ".github/workflows/portable.yml").read_text(), Loader=yaml.BaseLoader
        )
        self.assertEqual(set(workflow["on"]), {"push", "pull_request"})
        self.assertEqual(workflow["permissions"], {"contents": "read"})
        job = workflow["jobs"]["resources-and-ledger"]
        self.assertEqual(job["runs-on"], "ubuntu-24.04")
        commands = [step["run"] for step in job["steps"] if "run" in step]
        self.assertIn("python -m pip install -r requirements-dev.txt", commands)
        self.assertIn("bash tools/check-portable.sh", commands)
        self.assertLessEqual(int(job["timeout-minutes"]), 15)
        self.assertNotIn("upload-artifact", str(workflow))
        gate = (ROOT / "tools/check-portable.sh").read_text()
        self.assertIn("set -euo pipefail", gate)
        for required in [
            "ruff format --check",
            "ruff check",
            "compileall",
            "unittest discover",
            "git diff --check",
        ]:
            self.assertIn(required, gate)
        for forbidden in ["--exit-zero", "|| true", "continue-on-error"]:
            self.assertNotIn(forbidden, gate)

    def test_tooling_is_controlled_and_audit_has_all_initial_paths(self):
        config = tomllib.loads((ROOT / "pyproject.toml").read_text())
        self.assertEqual(config["tool"]["ruff"]["required-version"], "==0.14.3")
        self.assertEqual(config["tool"]["ruff"]["target-version"], "py311")
        requirements = (ROOT / "requirements-dev.txt").read_text()
        self.assertIn("ruff==0.14.3", requirements)
        self.assertIn("PyYAML==6.0.3", requirements)
        inventory = (ROOT / "docs/SOURCE-INVENTORY.md").read_text()
        paths = re.findall(
            r"^\| `([^`]+)` \| (\d+) \| (\d+) \| `([0-9a-f]{12})` \|$", inventory, re.MULTILINE
        )
        self.assertEqual(len(paths), 141)
        self.assertEqual(len({row[0] for row in paths}), 141)
        for path, _, _, _ in paths:
            self.assertTrue((ROOT / path).exists(), path)
        self.assertIn("osdbits/tools/gsreplay/gsreplay.c", {row[0] for row in paths})


if __name__ == "__main__":
    unittest.main()
