#!/usr/bin/env python3
"""Generate the Metallic roadmap dashboard from real GitHub issue states.

Standard library only. Task completion != running software/release verification.
Usage:
  GITHUB_TOKEN=... GITHUB_REPOSITORY=owner/repo python scripts/update_progress.py
  python scripts/update_progress.py --fixture tests/fixtures/issues.json --output-dir /tmp/metallic-progress
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import sys
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

from render_progress_svg import progress_svg

PHASES = [
    ("P0", "Research, architecture & dev tooling"),
    ("P1", "Accounts, backend & launcher"),
    ("P2", "Playable multiplayer + VR"),
    ("P3", "Maker Pen, creator tools & CV3"),
    ("P4", "Moderation, marketplace & Discord"),
    ("P5", "Interoperability & platform ports"),
    ("P6", "Reliability, performance & launch"),
]
PREFIX_RE = re.compile(r"^\[(P[0-6])\]\s+", re.IGNORECASE)
API_TIMEOUT = 25


def issue_phase(issue: dict) -> str | None:
    match = PREFIX_RE.match(issue.get("title", ""))
    return match.group(1).upper() if match else None


def task_labels(issue: dict) -> set[str]:
    return {
        (label.get("name", "") if isinstance(label, dict) else str(label)).casefold()
        for label in issue.get("labels", [])
    }


def clean_md(value: str) -> str:
    """Neutralize Markdown syntax in issue titles inserted into lists/tables."""
    value = str(value).replace("\r", " ").replace("\n", " ")
    return re.sub(r"([\\\[\]\*\_\`|<>])", r"\\\1", value).strip()[:180]


def github_issues(repo: str, token: str, api_url: str) -> list[dict]:
    """Read all open and closed issues. Pull requests are filtered later."""
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repo):
        raise ValueError("Invalid GITHUB_REPOSITORY")
    if not token:
        raise ValueError("GITHUB_TOKEN is required to read private repositories")
    base = api_url.rstrip("/")
    collected: list[dict] = []
    for page in range(1, 51):  # up to 5,000 issues; fail if pagination grows
        url = f"{base}/repos/{repo}/issues?state=all&per_page=100&page={page}"
        request = Request(url, headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "Metallic-Progress-Tracker/1.0",
            "X-GitHub-Api-Version": "2022-11-28",
        })
        with urlopen(request, timeout=API_TIMEOUT) as response:
            data = json.load(response)
        if not isinstance(data, list):
            raise ValueError("Unexpected GitHub issues API response")
        collected.extend(data)
        if len(data) < 100:
            return collected
    raise RuntimeError("Too many issues; pagination safety limit exceeded")


def render_progress(issues: list[dict], repo: str) -> tuple[str, dict]:
    """Return Markdown and a machine-readable JSON snapshot."""
    genuine = [issue for issue in issues if "pull_request" not in issue]
    by_phase: dict[str, list[dict]] = {phase: [] for phase, _ in PHASES}
    untracked: list[dict] = []
    for issue in genuine:
        phase = issue_phase(issue)
        if phase:
            by_phase[phase].append(issue)
        else:
            untracked.append(issue)

    counts = {}
    blocked: list[dict] = []
    active: list[dict] = []
    planned: list[dict] = []
    complete: list[dict] = []
    total = 0
    closed_total = 0

    for phase, title in PHASES:
        tasks = by_phase[phase]
        closed = [issue for issue in tasks if issue.get("state") == "closed"]
        opened = [issue for issue in tasks if issue.get("state") != "closed"]
        total += len(tasks)
        closed_total += len(closed)
        percent = round(100 * len(closed) / len(tasks), 1) if tasks else 0.0
        counts[phase] = {
            "name": title,
            "total": len(tasks),
            "closed": len(closed),
            "open": len(opened),
            "percent_closed": percent,
        }
        complete.extend(closed)
        for issue in opened:
            labels = task_labels(issue)
            if "status:blocked" in labels or "blocked" in labels:
                blocked.append(issue)
            elif "status:in-progress" in labels or "in progress" in labels:
                active.append(issue)
            else:
                planned.append(issue)

    percent_all = round(100 * closed_total / total, 1) if total else 0.0
    last_activity = max(
        (issue.get("updated_at", "") for issue in genuine), default=""
    ) or "No issue activity yet"
    snapshot = {
        "schema_version": 1,
        "repository": repo,
        "basis": "closed phase-tagged GitHub issues, not verified releases",
        "last_issue_activity_utc": last_activity,
        "overall": {
            "total": total,
            "closed": closed_total,
            "open": total - closed_total,
            "percent_closed": percent_all,
            "blocked": len(blocked),
            "in_progress": len(active),
            "not_started": len(planned),
            "untracked": len(untracked),
        },
        "phases": counts,
        "released": False,  # no assertion of verified releases from issues
    }

    lines = [
        "# Metallic-RC — Live Build Progress",
        "",
        "> **This dashboard is generated automatically from GitHub Issues.** "
        "A closed issue is a completed task, **not** proof of a tested executable, "
        "released multiplayer game, or production readiness. Releases must be "
        "verified separately using CI artifacts and acceptance tests.",
        "",
        f"**Repository:** [\`{repo}\`](https://github.com/{repo})",
        f"**Last tracked issue activity (UTC):** {last_activity}  ",
        "**Update trigger:** issue changes, manual dispatch, daily reconciliation, and relevant workflow changes.",
        "",
        "## Overall issue completion",
        "",
        f"**{closed_total}/{total} scoped issues closed ({percent_all:.1f}%).**",
        "",
        f"**Open:** {total - closed_total} · **In progress:** {len(active)} "
        f"· **Blocked:** {len(blocked)} · **Untracked:** {len(untracked)}",
        "",
        "| Phase | Closed/total | Issue completion |",
        "| --- | ---: | ---: |",
    ]
    for phase, title in PHASES:
        item = counts[phase]
        pct = item["percent_closed"]
        bar = "▰" * round(pct / 10) + "▱" * (10 - round(pct / 10))
        lines.append(
            f"| **{phase}** — {title} | {item['closed']}/{item['total']} | "
            f"\`{bar}\` {pct:.1f}% |"
        )

    def issue_line(issue: dict) -> str:
        title = clean_md(issue.get("title", "Untitled issue"))
        number = issue.get("number", "?")
        try:
            number = int(number)
        except (TypeError, ValueError):
            number = "?"
        url = issue.get("html_url", f"https://github.com/{repo}/issues/{number}")
        # Restrict links to this repository; never trust issue-supplied URLs.
        expected_url = f"https://github.com/{repo}/issues/{number}"
        if url != expected_url:
            url = expected_url
        return f"- [#{number}: {title}]({url})"

    lines.extend([
        "",
        "## Actively worked tasks",
        "",
        *([issue_line(x) for x in sorted(active, key=lambda i: i.get("number", 0))[:20]]
          or ["No tasks currently carry the \`status:in-progress\` label."]),
        "",
        "## Blocked tasks",
        "",
        *([issue_line(x) for x in sorted(blocked, key=lambda i: i.get("number", 0))[:20]]
          or ["No phase-tagged issues are labeled \`status:blocked\`."]),
        "",
        "## Next open tasks",
        "",
        *([issue_line(x) for x in sorted(planned, key=lambda i: i.get("number", 0))[:15]]
          or ["No open unstarted phase-tagged issues."]),
        "",
        "## Recently closed tasks",
        "",
        *([issue_line(x) for x in sorted(
            complete, key=lambda i: i.get("closed_at") or "", reverse=True
        )[:12]] or ["No scoped issue has been closed yet."]),
        "",
        "## What this does and does not measure",
        "",
        "- **Measures:** task closures, open tasks, status labels, milestone distribution.",
        "- **Does not measure:** code quality, server performance, actual builds, release status, player counts, or feature parity. Those require independent test evidence.",
        "- Every phase issue must be titled \`[P0]\` through \`[P6]\` to count.",
        "- Add \`status:in-progress\` or \`status:blocked\` label for a task state (labels are optional).",
        "- A reopened issue automatically lowers the completion count.",
        "- The percentage may decrease when new scope is added; it is not a predicted delivery date.",
        "",
        "[Engineering roadmap](ROADMAP.md) · "
        f"[Issues](https://github.com/{repo}/issues) · "
        f"[Pull requests](https://github.com/{repo}/pulls) · "
        "[Machine-readable progress](progress.json)",
        "",
    ])
    return "\n".join(lines), snapshot


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, help="Read GitHub API JSON from fixture")
    parser.add_argument("--output-dir", type=Path, default=Path("docs"))
    args = parser.parse_args()
    repo = os.environ.get("GITHUB_REPOSITORY", "oncoursemail/Metallic-RC")
    try:
        if args.fixture:
            issues = json.loads(args.fixture.read_text(encoding="utf-8"))
        else:
            issues = github_issues(
                repo, os.environ.get("GITHUB_TOKEN", ""),
                os.environ.get("GITHUB_API_URL", "https://api.github.com")
            )
        if not isinstance(issues, list):
            raise ValueError("Issue payload must be a JSON list")
        md, snapshot = render_progress(issues, repo)
        args.output_dir.mkdir(parents=True, exist_ok=True)
        (args.output_dir / "PROGRESS.md").write_text(md, encoding="utf-8")
        (args.output_dir / "progress.json").write_text(
            json.dumps(snapshot, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        (args.output_dir / "progress.svg").write_text(
            progress_svg(snapshot), encoding="utf-8"
        )
        print(f"Updated progress: {snapshot['overall']['closed']}/"
              f"{snapshot['overall']['total']} closed; {snapshot['overall']['percent_closed']}%")
        return 0
    except (ValueError, OSError, HTTPError, URLError, RuntimeError) as error:
        print(f"Failed to update roadmap: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
