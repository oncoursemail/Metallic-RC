#!/usr/bin/env python3
"""Render a sober, accessible metallic-themed SVG directly from progress.json.

Offline/stdlib only. SVG has no scripts, external fonts, animated content or links.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
from xml.sax.saxutils import escape

PHASE_KEYS = tuple(f"P{i}" for i in range(7))
BG = "#0E0F12"
PANEL = "#17181D"
BORDER = "#393A43"
TEXT = "#F0EEF1"
SUB = "#BAB8C2"
ACCENT = "#AA8CC5"
BAR_BG = "#2B2B33"
GREEN = "#82BFA3"


def fmt_pct(value: object) -> str:
    try:
        number = float(value)
    except (TypeError, ValueError, OverflowError):
        number = 0.0
    if number != number or not -float("inf") < number < float("inf"):
        number = 0.0
    return f"{max(0.0, min(100.0, number)):.1f}"


def num(value: object) -> int:
    try:
        return max(0, int(value))
    except (TypeError, ValueError, OverflowError):
        return 0


def safe_text(value: object) -> str:
    clean = str(value).replace("\n", " ").replace("\r", " ").strip()[:80]
    return escape(clean, {'"': "&quot;", "'": "&apos;"})


def progress_svg(snapshot: dict) -> str:
    if not isinstance(snapshot, dict):
        raise ValueError("expected progress snapshot dictionary")
    overall = snapshot.get("overall") or {}
    if not isinstance(overall, dict):
        raise ValueError("expected overall progress dictionary")
    phases = snapshot.get("phases") or {}
    if not isinstance(phases, dict):
        raise ValueError("expected phases dictionary")

    total, done = num(overall.get("total")), num(overall.get("closed"))
    done = min(done, total)
    pct = done * 100.0 / total if total else 0.0
    pct_label = f"{pct:.1f}"
    updated = safe_text(snapshot.get("last_issue_activity_utc") or "NO ISSUE UPDATES")
    groups = []
    for idx, key in enumerate(PHASE_KEYS):
        phase = phases.get(key) or {}
        if not isinstance(phase, dict):
            phase = {}
        count = num(phase.get("total"))
        closed = min(count, num(phase.get("closed")))
        local_pct = closed * 100 / count if count else 0.0
        x = 64 + (idx // 4) * 538
        y = 276 + (idx % 4) * 53
        label = safe_text(phase.get("name", f"Phase {key}"))
        groups.extend([
            f'<text x="{x}" y="{y}" fill="{ACCENT}" font-family="Segoe UI,Arial,sans-serif" font-weight="700" font-size="12" letter-spacing=".8">{key}</text>',
            f'<text x="{x+40}" y="{y}" fill="{TEXT}" font-family="Segoe UI,Arial,sans-serif" font-weight="500" font-size="14">{label}</text>',
            f'<text x="{x+490}" y="{y}" text-anchor="end" fill="{SUB}" font-family="Segoe UI,Arial,sans-serif" font-size="13">{closed}/{count}</text>',
            f'<rect x="{x}" y="{y+13}" width="490" height="5" rx="2" fill="{BAR_BG}"/>',
            f'<rect x="{x}" y="{y+13}" width="{490*local_pct/100:.2f}" height="5" rx="2" fill="{ACCENT}"/>',
        ])

    return "\n".join([
        '<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="510" viewBox="0 0 1200 510" role="img" aria-labelledby="title desc">',
        f'<title id="title">Metallic Studios development issue progress: {done} of {total} verified tasks closed ({pct_label} percent)</title>',
        f'<desc id="desc">Roadmap issue completion only, not a playable game or verified release. Data last changed {updated}.</desc>',
        f'<rect width="1200" height="510" fill="{BG}"/>',
        f'<rect x="12" y="12" width="1176" height="486" rx="8" fill="{PANEL}" stroke="{BORDER}"/>',
        f'<rect x="12" y="12" width="7" height="486" fill="{ACCENT}"/>',
        f'<path d="M32 132 H1168 M32 251 H1168 M32 464 H1168" stroke="{BORDER}" stroke-width="1" />',
        f'<text x="64" y="56" fill="{SUB}" font-size="13" font-family="Segoe UI,Arial,sans-serif" letter-spacing="4" font-weight="700">METALLIC  /  DEVELOPMENT</text>',
        f'<text x="64" y="108" fill="{TEXT}" font-size="38" font-family="Segoe UI,Arial,sans-serif" font-weight="700">Building the world.</text>',
        f'<text x="1142" y="72" text-anchor="end" fill="{ACCENT}" font-size="13" font-family="Segoe UI,Arial,sans-serif" letter-spacing="2">ROADMAP 2026</text>',
        f'<text x="64" y="170" fill="{SUB}" font-size="14" font-family="Segoe UI,Arial,sans-serif">TRACKED ENGINEERING TASKS</text>',
        f'<text x="64" y="215" fill="{TEXT}" font-size="42" font-family="Segoe UI,Arial,sans-serif" font-weight="700">{done}<tspan fill="{SUB}" font-size="23"> / {total} COMPLETE</tspan></text>',
        f'<text x="1142" y="194" fill="{TEXT}" text-anchor="end" font-size="36" font-family="Segoe UI,Arial,sans-serif" font-weight="700">{pct_label}%</text>',
        f'<rect x="64" y="230" width="1078" height="8" rx="3" fill="{BAR_BG}"/>',
        f'<rect x="64" y="230" width="{1078*pct/100:.2f}" height="8" rx="3" fill="{ACCENT}"/>',
        *groups,
        f'<circle cx="66" cy="484" r="3" fill="{GREEN}"/>',
        f'<text x="80" y="488" fill="{SUB}" font-size="11" font-family="Segoe UI,Arial,sans-serif">Automated from GitHub Issues · Last issue activity {updated} · No verified game release</text>',
        '</svg>',
        ''
    ])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--snapshot", type=Path, default=Path("docs/progress.json"))
    parser.add_argument("--output", type=Path, default=Path("docs/progress.svg"))
    args = parser.parse_args()
    snapshot = json.loads(args.snapshot.read_text(encoding="utf-8"))
    svg = progress_svg(snapshot)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(svg, encoding="utf-8")
    print(f"Generated {args.output} from {args.snapshot}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
