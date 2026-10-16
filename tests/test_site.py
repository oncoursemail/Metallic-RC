"""Static site structure tests. Not a substitute for browser screenshot review."""
from html.parser import HTMLParser
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1] / "docs" / "site"


class InspectMarkup(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids = []
        self.references = []

    def handle_starttag(self, tag, attrs):
        fields = dict(attrs)
        if "id" in fields:
            self.ids.append(fields["id"])
        if tag == "a":
            self.references.append(fields.get("href", ""))


class SiteStructureTests(unittest.TestCase):
    def test_markup_ids_unique_and_anchors_exist(self):
        parser = InspectMarkup()
        parser.feed((ROOT / "index.html").read_text(encoding="utf-8"))
        self.assertEqual(len(parser.ids), len(set(parser.ids)), "Duplicate IDs")
        self.assertIn("main", parser.ids)
        self.assertIn("progress-panel", parser.ids)
        for anchor in parser.references:
            if anchor.startswith("#"):
                self.assertIn(anchor[1:], parser.ids, "Broken local anchor: " + anchor)

    def test_no_fake_downloads_or_fabricated_live_count(self):
        html = (ROOT / "index.html").read_text(encoding="utf-8")
        self.assertIn("No public game download yet", html)
        self.assertIn("PRE-ALPHA / NO BUILD", html)
        self.assertNotIn("10,000 players online", html)
        self.assertNotIn('id="download-button"', html)

    def test_data_bindings_exist(self):
        html = (ROOT / "index.html").read_text(encoding="utf-8")
        js = (ROOT / "app.js").read_text(encoding="utf-8")
        for name in ("main-meter", "main-fill", "phases", "count-closed",
                     "count-total", "overall-percent", "progress-note", "progress-panel"):
            self.assertIn('id="' + name + '"', html)
            self.assertIn(name, js)
        self.assertIn("../progress.json", js)
        self.assertIn("notActualGamePlayers", (Path(__file__).parent / "load" / "simulated_players.py").read_text())

    def test_responsive_and_motion_accommodations(self):
        css = (ROOT / "styles.css").read_text(encoding="utf-8")
        self.assertIn("prefers-reduced-motion", css)
        self.assertRegex(css, r"@media\s*\(max-width:\s*760px\)")
        self.assertIn(":focus-visible", css)
        self.assertIn("--text:", css)


if __name__ == "__main__":
    unittest.main()
