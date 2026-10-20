"""Reject unsupported owned-icon styles before they reach runtime data."""
import itertools
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import generate_desktop_icons as generator


class StrokeStyleContract(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="motion-icon-contract-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.source = self.root / "fixture.svg"

    def write_svg(self, attributes):
        self.source.write_text(
            '<svg xmlns="http://www.w3.org/2000/svg" width="28" height="28">'
            f'<path d="M 2 2 L 20 20" {attributes}/></svg>',
            encoding="utf-8",
        )

    def test_supported_styles_and_defaults_preserve_generated_data(self):
        self.write_svg('fill="none" stroke="currentColor"')
        primitive = generator.compile_svg(self.source)["shapes"][0]
        self.assertEqual((primitive["cap"], primitive["join"]), ("butt", "miter"))
        for cap, join in itertools.product(
            ("butt", "round", "square"), ("miter", "round", "bevel")
        ):
            with self.subTest(cap=cap, join=join):
                self.write_svg(
                    'fill="none" stroke="currentColor" '
                    f'stroke-linecap="{cap}" stroke-linejoin="{join}"'
                )
                primitive = generator.compile_svg(self.source)["shapes"][0]
                self.assertEqual((primitive["cap"], primitive["join"]), (cap, join))
                self.assertEqual(primitive["commands"], [["M", 2., 2.], ["L", 20., 20.]])

    def test_unsupported_styles_are_rejected_even_without_visible_stroke(self):
        for attribute, value in (
            ("stroke-linecap", "unsupported-cap"),
            ("stroke-linecap", ""),
            ("stroke-linejoin", "arcs"),
            ("stroke-linejoin", "miter-clip"),
        ):
            for stroke in ("none", "currentColor"):
                with self.subTest(attribute=attribute, value=value, stroke=stroke):
                    self.write_svg(f'stroke="{stroke}" {attribute}="{value}"')
                    with self.assertRaises(ValueError) as raised:
                        generator.compile_svg(self.source)
                    message = str(raised.exception)
                    self.assertIn(str(self.source), message)
                    self.assertIn(attribute, message)
                    self.assertIn(repr(value), message)

    def test_generation_failure_preserves_output_and_reports_only_to_stderr(self):
        (self.root / "manifest.json").write_text(
            json.dumps({"icons": [{"name": "fixture"}]}), encoding="utf-8"
        )
        destination = self.root / "paths.json"
        previous = b"previous generated output\n"
        for attribute, value in (
            ("stroke-linecap", "unsupported-cap"),
            ("stroke-linejoin", "unsupported-join"),
        ):
            self.write_svg(f'stroke="currentColor" {attribute}="{value}"')
            for check in (False, True):
                with self.subTest(attribute=attribute, check=check):
                    destination.write_bytes(previous)
                    # Exercise the actual CLI entry point with an isolated icon root.
                    script = (
                        "import sys; from pathlib import Path; "
                        "import generate_desktop_icons as tool; "
                        "tool.ICONS=Path(sys.argv.pop(1)); tool.main()"
                    )
                    result = subprocess.run(
                        [sys.executable, "-c", script, str(self.root), *(["--check"] if check else [])],
                        cwd=Path(generator.__file__).parent,
                        capture_output=True, text=True, encoding="utf-8", timeout=30,
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(result.stdout, "")
                    self.assertIn(attribute, result.stderr)
                    self.assertIn(str(self.source), result.stderr)
                    self.assertIn(value, result.stderr)
                    self.assertEqual(destination.read_bytes(), previous)


if __name__ == "__main__":
    unittest.main()
