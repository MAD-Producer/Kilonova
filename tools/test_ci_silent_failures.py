"""Regression coverage for source-template and release-scan failure paths."""
import contextlib
import importlib.util
import io
import shutil
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from source_templates import replace_required
import ci_desktop
import validate_provenance


class SilentFailureTests(unittest.TestCase):
    def test_required_replacement_rejects_missing_or_ambiguous_template(self):
        self.assertEqual(replace_required("a target b", "target", "fixed"), "a fixed b")
        for source in ["changed", "target target"]:
            with self.assertRaises(ValueError):
                replace_required(source, "target", "fixed")

    def test_real_creative_templates_keep_the_same_generated_shaders(self):
        # Import computes templates but does not publish generated library files.
        path = Path(__file__).with_name("generate_creative_library.py")
        spec = importlib.util.spec_from_file_location("creative_check", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.assertGreater(len(module.EFFECTS), 0)
        shaders = {definition["id"]: shader for definition, shader, _ in module.EFFECTS}
        self.assertIn("for(var ray:i32=0;ray<4", shaders["glint"])
        self.assertNotIn("$", shaders["glint"])
        self.assertIn("edge_light", shaders["edge_rays"])

    def test_real_spatial_templates_build_without_publishing(self):
        path = Path(__file__).with_name("generate_spatial_library.py")
        spec = importlib.util.spec_from_file_location("spatial_check", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory:
            library = Path(directory)
            (library / "legacy").mkdir()
            (library / "shaders").mkdir()
            shutil.copy2(
                module.LIB / "legacy/core-effects-1.3.0.msfx",
                library / "legacy/core-effects-1.3.0.msfx",
            )
            with mock.patch.object(module, "LIB", library), contextlib.redirect_stdout(io.StringIO()):
                module.build()
            shader = (library / "shaders/shake.wgsl").read_text(encoding="utf-8")
            self.assertIn("if any(q<fx.input_region.xy)", shader)

    def test_style_guard_scans_nested_panel_sources(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            nested = root / "properties" / "deeper"
            nested.mkdir(parents=True)
            source = nested / "row.rs"
            source.write_text("let color = palette::FOCUS;", encoding="utf-8")
            with self.assertRaises(RuntimeError):
                ci_desktop.verify_style_consumers(root)
            source.write_text("let color = styles.colors().FOCUS;", encoding="utf-8")
            ci_desktop.verify_style_consumers(root)

    def test_invalid_utf8_release_text_cannot_bypass_provenance_guard(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = Path("website/index.html")
            (root / path).parent.mkdir(parents=True)
            (root / path).write_bytes(bytes([255]) + b" Adobe")
            with mock.patch.object(validate_provenance, "ROOT", root), mock.patch.object(
                validate_provenance, "tracked_paths", return_value=[path]
            ), contextlib.redirect_stderr(io.StringIO()) as errors:
                self.assertEqual(validate_provenance.main(), 1)
            self.assertIn("not valid UTF-8", errors.getvalue())
            self.assertIn(path.as_posix().split("/")[-1], errors.getvalue())


if __name__ == "__main__":
    unittest.main()
