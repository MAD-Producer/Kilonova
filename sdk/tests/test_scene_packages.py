"""Regenerate actual scene/particle packages with different host ZIP defaults."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk/tests/support"))
import generate_particle_library as particle
import generate_scene_library as scene

spec = importlib.util.spec_from_file_location("scene_sdk_packer", ROOT / "sdk/motion-studio-plugin/scripts/pack_msfx.py")
packer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packer)


class ScenePackageTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="motion-scene-packages-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.validator = packer.find_validator(None)

    def host_defaults(self, system, permissions):
        original = zipfile.ZipInfo.__init__
        def initialize(info, *args, **kwargs):
            original(info, *args, **kwargs)
            info.create_system = system
            info.external_attr = permissions << 16
        return mock.patch.object(zipfile.ZipInfo, "__init__", initialize)

    def test_sdk_output_bytes_ignore_host_metadata_input_order_and_compressor_backend(self):
        files = [("shaders/test.wgsl", b"shader payload"), ("manifest.json", b'{"name":"test"}'),
                 ("assets/payload.png", bytes(range(256)))]
        original = zipfile._get_compressor
        def stored_only(method, level=None):
            self.assertEqual(method, zipfile.ZIP_STORED, "a compression backend would change archive identity")
            return original(method, level)
        outputs = []
        for system, permissions, backend in [(0, 0o600, "zlib"), (3, 0o100755, "zlib-ng")]:
            path = self.root / f"{system}.msfx"
            with self.host_defaults(system, permissions), mock.patch("zipfile._get_compressor", side_effect=stored_only), \
                 mock.patch("zipfile.zlib.ZLIB_RUNTIME_VERSION", backend):
                packer.write_archive(path, files if system == 0 else list(reversed(files)))
            outputs.append(path.read_bytes())
        self.assertEqual(outputs[0], outputs[1])
        self.assertEqual(hashlib.sha256(outputs[0]).hexdigest(), hashlib.sha256(outputs[1]).hexdigest())
        with zipfile.ZipFile(self.root / "0.msfx") as archive:
            self.assertEqual(archive.namelist(), sorted(name for name, _ in files))
            for info in archive.infolist():
                self.assertEqual((info.create_system, info.external_attr, info.compress_type),
                                 (3, 0o100644 << 16, zipfile.ZIP_STORED))
                self.assertEqual(info.date_time, (1980, 1, 1, 0, 0, 0))
                self.assertEqual((info.extra, info.comment), (b"", b""))

    def test_actual_generators_match_published_bytes_with_windows_and_linux_inputs(self):
        outputs = {"scene": [], "particle": []}
        for system, permissions in [(0, 0o600), (3, 0o100755)]:
            sources = self.root / f"sources-{system}"
            ui = sources / "ui"
            ui.mkdir(parents=True)
            for path in (scene.DEST / "ui").iterdir():
                if path.is_file():
                    data = path.read_bytes().replace(b"\r\n", b"\n")
                    target = ui / path.name
                    target.write_bytes(data.replace(b"\n", b"\r\n") if system == 0 else data)
                    os.utime(target, (1000000000 + system, 1000000000 + system))
            shader = sources / "sprite.wgsl"
            data = (particle.DEST / "shaders/sprite.wgsl").read_bytes().replace(b"\r\n", b"\n")
            shader.write_bytes(data.replace(b"\n", b"\r\n") if system == 0 else data)
            with self.host_defaults(system, permissions):
                scene_report = scene.generate(self.root / f"scene-{system}", self.validator, ui_source=sources)
                particle_report = particle.generate(self.root / f"particle-{system}", self.validator, shader_source=shader)
            for name, report in [("scene", scene_report), ("particle", particle_report)]:
                data = Path(report["path"]).read_bytes()
                outputs[name].append(data)
                self.assertEqual(report["sha256"], hashlib.sha256(data).hexdigest())
                self.assertEqual(report["compression_backend"], "none")
                self.assertEqual(report["validation"], "sdk")
        for name, module in [("scene", scene), ("particle", particle)]:
            self.assertEqual(outputs[name][0], outputs[name][1], name)
            self.assertEqual(outputs[name][0], (module.DEST / f"{name}-effects.msfx").read_bytes(),
                             f"{name} published package is stale: regenerate with a new version")

    def test_package_file_sets_equal_the_real_loader_declarations(self):
        for module, name, version in [(scene, "scene", "1.1.1"), (particle, "particle", "1.0.1")]:
            plan = json.loads(packer.validate(self.validator, "plan", str(module.DEST / "manifest.json")))
            package = module.DEST / f"{name}-effects.msfx"
            with zipfile.ZipFile(package) as archive:
                self.assertEqual(set(archive.namelist()), {entry["name"] for entry in plan["files"]})
                self.assertEqual(json.loads(archive.read("manifest.json"))["version"], version)
            packer.validate(self.validator, "check", str(package))

    def test_legacy_packages_keep_published_hashes_and_loader_compatibility(self):
        for path, digest in [
            (scene.DEST / "legacy/scene-effects-1.1.0.msfx", "c8c312ed6239fe65760e7c41811a9c0a1db2a548b8abe142230377bb57830bc1"),
            (particle.DEST / "legacy/particle-effects-1.0.0.msfx", "37047f16cb9306b7e15d73c343f4f76fbf26c4ddfee4e1c2d3fe973939760e8b"),
            (scene.DEST / "legacy/scene-effects-1.0.0.msfx", "ffd6d7ddbff861072da76dd7f84f072a7b01b187f8488e59e88e4bc468c20724"),
            (ROOT / "crates/motion-effects/builtin-library/builtin-effects.msfx", "d492322ae4ee5254b008bc734def9d686313e8e3fece8dd9d9597c8e774dd821"),
        ]:
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), digest)
            packer.validate(self.validator, "check", str(path))

    def test_generator_check_cli_detects_stale_bytes_without_changing_the_checkout(self):
        for module, name in [(scene, "scene"), (particle, "particle")]:
            env = os.environ | {"MOTION_EFFECT_VALIDATOR": str(self.validator)}
            script = ROOT / "sdk/tests/support" / f"generate_{name}_library.py"
            before = (module.DEST / f"{name}-effects.msfx").read_bytes()
            result = subprocess.run([sys.executable, str(script), "--check"], env=env,
                                    check=True, capture_output=True, text=True, encoding="utf-8")
            self.assertTrue(json.loads(result.stdout)["checked"])
            self.assertEqual((module.DEST / f"{name}-effects.msfx").read_bytes(), before)
        clone = self.root / "clone"
        for name in ["sdk/tests/support/generate_scene_library.py", "sdk/motion-studio-plugin/scripts/pack_msfx.py"]:
            destination = clone / name
            destination.parent.mkdir(parents=True)
            shutil.copy2(ROOT / name, destination)
        source = clone / "crates/motion-effects/tests/fixtures/scene-library"
        shutil.copytree(scene.DEST, source)
        package = source / "scene-effects.msfx"
        before = package.read_bytes() + b"stale bytes"
        package.write_bytes(before)
        result = subprocess.run([sys.executable, "-O", str(clone / "sdk/tests/support/generate_scene_library.py"), "--check"],
                                env=env, capture_output=True, text=True, encoding="utf-8")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("scene package is stale", result.stderr)
        self.assertEqual(package.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
