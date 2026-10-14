import importlib.util
import hashlib
import os
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile
from unittest import mock

SDK = Path(__file__).resolve().parents[1]
SKILL = SDK / "motion-studio-plugin"
spec = importlib.util.spec_from_file_location("pack_msfx", SKILL / "scripts/pack_msfx.py")
packer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packer)


class PackageWorkflowTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.source = self.root / "effect"
        shutil.copytree(SKILL / "assets/effect-template", self.source)
        self.output = self.root / "gain.msfx"

    def tearDown(self):
        self.temporary.cleanup()

    def manifest(self, change):
        path = self.source / "manifest.json"
        value = json.loads(path.read_text(encoding="utf-8"))
        change(value)
        path.write_text(json.dumps(value), encoding="utf-8")

    def test_pack_is_repeatable_and_includes_only_declared_files(self):
        (self.source / "private.txt").write_text("do not publish")
        first = packer.pack(self.source, self.output)
        with zipfile.ZipFile(self.output) as package:
            self.assertEqual(set(package.namelist()), {"manifest.json", "shaders/gain.wgsl"})
            manifest = json.loads(package.read("manifest.json"))
            self.assertEqual(manifest["effects"][0]["native_editor"]["protocol"], 1)
        self.assertEqual(first["sha256"], packer.pack(self.source, self.output)["sha256"])

    def test_switching_a_published_deflate_package_to_stored_requires_a_new_version(self):
        with zipfile.ZipFile(self.output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for name in ["manifest.json", "shaders/gain.wgsl"]:
                archive.writestr(name, (self.source / name).read_bytes())
        before = self.output.read_bytes()
        with self.assertRaisesRegex(ValueError, "increment the version"):
            packer.pack(self.source, self.output)
        self.assertEqual(self.output.read_bytes(), before)
        self.assertFalse(list(self.root.glob(".msfx-*.tmp")))
        self.manifest(lambda value: value.update(version="0.3.0"))
        result = packer.pack(self.source, self.output)
        self.assertEqual(result["version"], "0.3.0")
        self.assertEqual(result["archive_format"], "stored-v1")
        self.assertEqual(result["compression_backend"], "none")
        with zipfile.ZipFile(self.output) as archive:
            self.assertTrue(all(info.compress_type == zipfile.ZIP_STORED for info in archive.infolist()))

    def test_changed_valid_sources_cannot_replace_the_same_plugin_version(self):
        packer.pack(self.source, self.output)
        before = self.output.read_bytes()
        self.manifest(lambda value: value.update(name="Changed valid package"))
        with self.assertRaisesRegex(ValueError, "same plugin version"):
            packer.pack(self.source, self.output)
        self.assertEqual(self.output.read_bytes(), before)

    def test_existing_output_identity_check_bounds_manifest_inflation(self):
        with zipfile.ZipFile(self.output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("manifest.json", b" " * (512 * 1024 + 1))
        before = self.output.read_bytes()
        with self.assertRaisesRegex(ValueError, "existing manifest exceeds"):
            packer.pack(self.source, self.output)
        self.assertEqual(self.output.read_bytes(), before)

    def test_invalid_reference_preserves_previous_package(self):
        packer.pack(self.source, self.output)
        before = self.output.read_bytes()
        self.manifest(lambda value: value["effects"][0]["passes"][0].update(shader="../private.wgsl"))
        with self.assertRaises(ValueError):
            packer.pack(self.source, self.output)
        self.assertEqual(self.output.read_bytes(), before)

    def test_case_aliases_are_rejected(self):
        self.manifest(lambda value: value["effects"][0]["passes"].append({"shader":"shaders/GAIN.wgsl","entry":"main_fx"}))
        with self.assertRaisesRegex(ValueError, "case-insensitive"):
            packer.pack(self.source, self.output)

    def test_resource_resolution_must_stay_within_package(self):
        outside = self.root / "outside.wgsl"
        outside.write_text("not a package resource")
        target = self.source / "shaders/gain.wgsl"
        target.unlink()
        try:
            target.symlink_to(outside)
        except OSError as error:
            if sys.platform != "win32":
                raise
            # Directory junctions exercise the same resolved-path contract
            # without requiring Windows' symbolic-link privilege.
            directory = self.root / "external-files"
            directory.mkdir()
            shutil.copyfile(outside, directory / "outside.wgsl")
            junction = self.source / "shaders" / "external"
            self.assertTrue(directory.resolve().is_relative_to(self.root.resolve()))
            self.assertTrue(junction.resolve().is_relative_to(self.root.resolve()))
            quote = lambda path: "'" + str(path).replace("'", "''") + "'"
            subprocess.run(["powershell", "-NoProfile", "-NonInteractive", "-Command",
                f"New-Item -ItemType Junction -Path {quote(junction)} -Target {quote(directory)} -ErrorAction Stop | Out-Null"], check=True)
            self.manifest(lambda value: value["effects"][0]["passes"][0].update(shader="shaders/external/outside.wgsl"))
        with self.assertRaisesRegex(ValueError, "outside source"):
            packer.pack(self.source, self.output)

    def test_nonfinite_json_and_non_msfx_output_are_rejected(self):
        self.manifest(lambda value: value["effects"][0]["params"][0].update(max=float("inf")))
        with self.assertRaises(ValueError):
            packer.pack(self.source, self.output)
        shutil.copyfile(SKILL / "assets/effect-template/manifest.json", self.source / "manifest.json")
        with self.assertRaisesRegex(ValueError, "output must use"):
            packer.pack(self.source, self.root / "gain.zip")

    def test_missing_validator_does_not_replace_published_output(self):
        packer.pack(self.source, self.output)
        before = self.output.read_bytes()
        with self.assertRaises(OSError):
            packer.pack(self.source, self.output, self.root / "missing-validator")
        self.assertEqual(self.output.read_bytes(), before)
        self.assertFalse(list(self.root.glob(".msfx-*.tmp")))


    def editor(self, files):
        def change(value):
            effect = value["effects"][0]
            effect.pop("native_editor", None)
            effect["required_capabilities"] = ["single_frame", "color_profile"]
            effect["editor"] = {"id":"example.editor", "protocol":1, "entry":files[0], "files":files, "title":"Editor"}
        self.manifest(change)

    def test_rust_plan_is_the_same_file_set_and_budgets_as_the_loader(self):
        validator = packer.find_validator(None)
        plan = json.loads(packer.validate(validator, "plan", str(self.source / "manifest.json")))
        self.assertEqual(plan["protocol"], 1)
        self.assertEqual(plan["manifestSha256"], hashlib.sha256((self.source / "manifest.json").read_bytes()).hexdigest())
        self.assertEqual({f["name"]:f["byteLimit"] for f in plan["files"]}, {"manifest.json":512*1024,"shaders/gain.wgsl":256*1024})
        result = packer.pack(self.source, self.output)
        self.assertEqual(result["validation"], "sdk")
        packer.validate(validator, "check", str(self.output))

    def test_utf8_byte_limits_and_root_shader_paths_are_rejected_by_both_paths(self):
        for name in ["shaders/"+"x"*250+".wgsl", "shaders/"+"界"*83+".wgsl", "evil.wgsl"]:
            with self.subTest(name=name):
                shutil.copyfile(SKILL / "assets/effect-template/manifest.json", self.source / "manifest.json")
                self.manifest(lambda value: value["effects"][0]["passes"][0].update(shader=name))
                self.assert_rejected_by_packer_and_loader()

    def assert_rejected_by_packer_and_loader(self):
        with self.assertRaises(ValueError):
            packer.pack(self.source, self.output)
        archive_path = self.root / "invalid.msfx"
        with zipfile.ZipFile(archive_path,"w") as archive:
            archive.write(self.source / "manifest.json", "manifest.json")
        with self.assertRaises(ValueError):
            packer.validate(packer.find_validator(None), "check", str(archive_path))

    def test_resource_and_editor_roles_are_shared_with_rust(self):
        for name in ["assets/logo.gif", "logo.png"]:
            with self.subTest(name=name):
                self.manifest(lambda value: value["effects"][0].update(resources=[name]))
                self.assert_rejected_by_packer_and_loader()
        for files in [["editor.html"], ["ui/index.html","ui/unsupported.exe"]]:
            with self.subTest(files=files):
                shutil.copyfile(SKILL / "assets/effect-template/manifest.json", self.source / "manifest.json")
                self.editor(files)
                self.assert_rejected_by_packer_and_loader()

    def test_editor_source_budget_applies_before_reading(self):
        self.editor(["ui/index.html","ui/editor.js"])
        (self.source / "ui").mkdir()
        (self.source / "ui/index.html").write_text("<html></html>")
        (self.source / "ui/editor.js").write_bytes(b" "*(256*1024+1))
        with self.assertRaisesRegex(ValueError, "SDK budget: ui/editor.js"):
            packer.pack(self.source, self.output)

    def test_complete_validation_preserves_published_output_for_invalid_png_and_shader(self):
        packer.pack(self.source, self.output)
        before = self.output.read_bytes()
        (self.source / "shaders/gain.wgsl").write_text("fn main_fx(p:vec2<f32>)->vec4<f32>{while true {} return vec4(0.0);}")
        with self.assertRaisesRegex(ValueError, "effect_tool check failed"):
            packer.pack(self.source, self.output)
        self.assertEqual(self.output.read_bytes(), before)
        shutil.copyfile(SKILL / "assets/effect-template/shaders/gain.wgsl", self.source / "shaders/gain.wgsl")
        (self.source / "assets").mkdir()
        (self.source / "assets/invalid.png").write_bytes(b"not png")
        self.manifest(lambda value: value["effects"][0].update(resources=["assets/invalid.png"]))
        with self.assertRaisesRegex(ValueError, "effect_tool check failed"):
            packer.pack(self.source, self.output)
        self.assertEqual(self.output.read_bytes(), before)
        self.assertFalse(list(self.root.glob(".msfx-*.tmp")))

    def test_changed_manifest_plan_cannot_publish_different_references(self):
        original = packer.validate
        def changed(validator, *args):
            result = original(validator, *args)
            if args[0] == "plan":
                self.manifest(lambda value: value.update(version="0.3.0"))
            return result
        with mock.patch.object(packer,"validate",side_effect=changed):
            with self.assertRaisesRegex(ValueError, "manifest changed"):
                packer.pack(self.source, self.output)
        self.assertFalse(self.output.exists())

    def test_missing_default_validator_never_downgrades_to_structure_only(self):
        with mock.patch.dict(os.environ,{"MOTION_EFFECT_VALIDATOR":str(self.root/"missing-tool")}):
            with self.assertRaises(OSError):
                packer.pack(self.source,self.output)
        self.assertFalse(self.output.exists())

    def test_cli_stdout_remains_machine_readable_with_default_validation(self):
        result = subprocess.run([sys.executable,str(SKILL/"scripts/pack_msfx.py"),str(self.source),str(self.output)],capture_output=True,text=True,encoding="utf-8",check=True)
        self.assertEqual(json.loads(result.stdout)["validation"],"sdk")


if __name__ == "__main__":
    unittest.main()
