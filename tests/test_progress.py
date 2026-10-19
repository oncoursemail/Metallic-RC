"""Regression tests for deterministic issue-driven roadmap counting."""
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from update_progress import issue_phase, render_progress, clean_md


REPO = "oncoursemail/Metallic-RC"


def issue(number, title, state="open", labels=None, pr=False):
    payload = {
        "number": number,
        "title": title,
        "state": state,
        "labels": [{"name": x} for x in (labels or [])],
        "updated_at": "2026-10-15T00:00:00Z",
        "closed_at": "2026-10-15T00:00:00Z" if state == "closed" else None,
        "html_url": f"https://github.com/{REPO}/issues/{number}",
    }
    if pr:
        payload["pull_request"] = {"url": "http://example.com"}
    return payload


class ProgressTests(unittest.TestCase):
    def test_empty_does_not_report_100_percent(self):
        md, value = render_progress([], REPO)
        self.assertEqual(value["overall"]["percent_closed"], 0)
        self.assertEqual(value["overall"]["total"], 0)
        self.assertIn("0/0 scoped issues closed", md)

    def test_only_phase_issues_not_prs_are_counted(self):
        payload = [
            issue(1, "[P0] Make initial toolchain", "closed"),
            issue(2, "[P0] Implement a test"),
            issue(3, "[P1] Authentication", labels=["status:in-progress"]),
            issue(4, "[P2] Blocked feature", labels=["status:blocked"]),
            issue(5, "[P0] Merged pull request", "closed", pr=True),
            issue(6, "General task"),
        ]
        md, value = render_progress(payload, REPO)
        self.assertEqual(value["overall"]["total"], 4)
        self.assertEqual(value["overall"]["closed"], 1)
        self.assertEqual(value["overall"]["percent_closed"], 25)
        self.assertEqual(value["overall"]["blocked"], 1)
        self.assertEqual(value["overall"]["in_progress"], 1)
        self.assertEqual(value["overall"]["untracked"], 1)
        self.assertIn("#3:", md)

    def test_reopened_issue_decreases_completion(self):
        first = [issue(1, "[P0] bootstrap", "closed")]
        second = [issue(1, "[P0] bootstrap", "open")]
        self.assertEqual(render_progress(first, REPO)[1]["overall"]["percent_closed"], 100)
        self.assertEqual(render_progress(second, REPO)[1]["overall"]["percent_closed"], 0)

    def test_case_insensitive_prefix(self):
        self.assertEqual(issue_phase(issue(1, "[p6] Launch")), "P6")
        self.assertIsNone(issue_phase(issue(2, "P6 missing brackets")))

    def test_title_injection_is_escaped(self):
        md, _ = render_progress([issue(1, "[P0] <script>alert(x)</script> | []")], REPO)
        self.assertIn(r"\<script\>", md)
        self.assertIn(r"\|", md)

    def test_deterministic_output(self):
        payload = [issue(9, "[P0] Sample", "closed")]
        self.assertEqual(render_progress(payload, REPO), render_progress(payload, REPO))


if __name__ == "__main__":
    unittest.main()
