"""Checks for branded SVG progress artwork and honest counts."""
from pathlib import Path
import sys
import unittest
import xml.etree.ElementTree as ET

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from render_progress_svg import progress_svg


class ProgressArtTests(unittest.TestCase):
    def test_empty_is_zero_not_fake_complete(self):
        svg = progress_svg({"overall": {"total": 0, "closed": 0}, "phases": {}})
        ET.fromstring(svg)
        self.assertIn("0.0%", svg)
        self.assertIn("0 / 0 COMPLETE", svg)
        self.assertIn("No verified game release", svg)

    def test_real_close_math(self):
        snap = {"overall": {"total": 40, "closed": 10}, "phases": {
            "P0": {"name": "Architecture", "closed": 5, "total": 10}
        }}
        svg = progress_svg(snap)
        root = ET.fromstring(svg)
        self.assertEqual(root.attrib["width"], "1200")
        self.assertIn("25.0%", svg)
        self.assertIn("10 / 40 COMPLETE", svg)

    def test_svg_safe_even_with_bad_source(self):
        svg = progress_svg({
            "overall": {"total": 3, "closed": 1000},
            "phases": {"P2": {"name": '<bad attr="x">&', "total": 1, "closed": 9}},
            "last_issue_activity_utc": '"><script>bad</script>&',
        })
        ET.fromstring(svg)
        self.assertNotIn("<script>", svg)
        self.assertIn("&lt;script&gt;", svg)
        self.assertIn("100.0%", svg)
        self.assertIn("&lt;bad", svg)

    def test_no_remote_deps_or_animation(self):
        svg = progress_svg({"overall": {"total": 34, "closed": 1}, "phases": {}})
        self.assertNotIn("http://", svg.replace('xmlns="http://www.w3.org/2000/svg"', ""))
        self.assertNotIn("<script", svg)
        self.assertNotIn("<animate", svg)
        self.assertIn("2.9%", svg)


if __name__ == "__main__":
    unittest.main()
