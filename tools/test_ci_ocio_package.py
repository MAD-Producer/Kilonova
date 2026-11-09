"""Pinned license bytes and synthetic architecture fixtures qualify packaging, not native execution."""
import copy
import json
from pathlib import Path
import tarfile
import unittest
from unittest.mock import patch
import zipfile

import package_desktop as desktop
import prepare_ocio as ocio
import test_ci_desktop_release as release_tests


class OcioPackageTest(unittest.TestCase):
    def setUp(self):
        self.release = release_tests.DesktopReleaseTest(methodName="runTest")
        self.release.setUp()
        self.addCleanup(self.release.doCleanups)
        self.root = self.release.root

    def manifest(self, platform):
        root = self.root / (platform + "-ocio")
        root.mkdir()
        binary = self.release.binary(platform)
        libraries = (["bin/OpenColorIO_2_6.dll", "bin/motion_ocio_bridge.dll"]
                     if platform == "windows-x86_64" else
                     ["lib/libOpenColorIO.2.6.dylib", "lib/libmotion_ocio_bridge.dylib"]
                     if platform.startswith("macos-") else
                     ["lib/libOpenColorIO.so.2.6", "lib/libmotion_ocio_bridge.so"])
        files, licenses = [], []
        for name in libraries:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(binary.read_bytes())
            files.append(dict(path=name, sha256=ocio.digest(path)))
        for item in ocio.sources(platform):
            name = "licenses/ocio/" + item["name"] + "-LICENSE.txt"
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            original = Path(__file__).parent / "tests/ocio_licenses" / path.name
            path.write_bytes(original.read_bytes())
            self.assertEqual(ocio.digest(path), item["license_sha256"])
            licenses.append(dict(path=name, sha256=ocio.digest(path)))
        manifest = root / "ocio-runtime.json"
        manifest.write_text(json.dumps(dict(schemaVersion=1, version="2.6.0", platform=platform,
            components=ocio.sources(platform), files=files, licenses=licenses,
            sourceLockSha256=ocio.digest(ocio.LOCK),
            bridgeSourceSha256=ocio.digest(Path(ocio.__file__).with_name("ocio-native") / "bridge.cpp"))))
        return manifest

    def test_every_platform_has_real_pinned_license_bytes_and_rejects_bad_inputs(self):
        for platform in desktop.PLATFORMS:
            with self.subTest(platform=platform):
                manifest = self.manifest(platform)
                document, files = ocio.runtime_payload(manifest, platform)
                self.assertEqual(len(files), 10 if platform == "macos-arm64" else 9)
                for mutate in (
                    lambda d: d.update(qualificationOnly="intentional fault fixture"),
                    lambda d: d["files"][0].update(path="../escaped.dll"),
                    lambda d: d["files"].append(d["files"][0]),
                    lambda d: d["files"].pop(),
                    lambda d: d["licenses"].pop(),
                    lambda d: d.update(platform="wrong"),
                    lambda d: d.update(bridgeSourceSha256="0" * 64),
                    lambda d: d["components"][0].update(commit="0" * 40),
                    lambda d: d["files"][0].update(sha256="0" * 64),
                ):
                    changed = copy.deepcopy(document)
                    mutate(changed)
                    manifest.write_text(json.dumps(changed))
                    with self.assertRaises(ValueError):
                        ocio.runtime_payload(manifest, platform)
                manifest.write_text(json.dumps(document))
                library = manifest.parent / document["files"][0]["path"]
                library.write_bytes(b"corrupt original native bytes")
                with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                    ocio.runtime_payload(manifest, platform)

    def test_packages_retain_native_layout_and_signing_updates_actual_hashes(self):
        records = []
        for platform in desktop.PLATFORMS:
            manifest = self.manifest(platform)
            original = json.loads(manifest.read_text())
            binary = self.release.binary(platform)
            report = self.release.build_report(binary, platform)
            def sign(command, **kwargs):
                if "--verify" not in command and Path(command[-1]).is_file():
                    path = Path(command[-1])
                    path.write_bytes(path.read_bytes() + b"synthetic signature byte change")
            if platform.startswith("macos-"):
                with patch.object(desktop.sys, "platform", "darwin"), patch.object(desktop.subprocess, "run", side_effect=sign):
                    info = desktop.package(binary, platform, report, self.release.folder, ocio_runtime=manifest)
            else:
                info = desktop.package(binary, platform, report, self.release.folder, ocio_runtime=manifest)
            records.append(info)
            self.assertEqual(json.loads(manifest.read_text()), original, "signing must not modify the source prefix")
            archive = self.release.folder / info["archive"]
            with (tarfile.open(archive) if platform == "linux-x86_64" else zipfile.ZipFile(archive)) as bundle:
                read = bundle.extractfile if platform == "linux-x86_64" else bundle.open
                with read(info["ocio"]["manifest"]) as stream:
                    native = json.load(stream)
                ocio.runtime_identity(native, platform)
                with read("OCIO-SOURCE.json") as stream:
                    self.assertEqual(json.load(stream), original)
                if platform.startswith("macos-"):
                    self.assertNotEqual(native["files"], original["files"])
            self.assertTrue(info["capabilities"]["ocioNative"])
        release = {k: records[0][k] for k in ("commit", "ref", "kind", "run", "versionName")}
        self.assertEqual(len(desktop.collect_desktop_assets(self.release.folder, release)), 4)
        report = self.release.folder / "build-info-windows-x86_64.json"
        info = json.loads(report.read_text())
        info["runtimeFiles"] = [e for e in info["runtimeFiles"] if "motion_ocio_bridge" not in e["path"]]
        report.write_text(json.dumps(info))
        with self.assertRaisesRegex(ValueError, "OCIO runtime inventory incomplete"):
            desktop.collect_desktop_assets(self.release.folder, release)

    def test_native_qualification_gates_every_build_and_release_receives_its_manifest(self):
        workflow = release_tests.yaml.load((desktop.ROOT / ".github/workflows/ci.yml").read_text(),
                                          Loader=release_tests.yaml.BaseLoader)
        steps = workflow["jobs"]["desktop"]["steps"]
        prepare = next(s for s in steps if s.get("name") == "Prepare pinned native OCIO runtime")
        qualify = next(s for s in steps if s.get("name") == "Qualify native OCIO against independent references")
        package = next(s for s in steps if s.get("name") == "Package desktop release archive")
        self.assertNotIn("if", prepare)
        self.assertNotIn("if", qualify)
        self.assertNotIn("continue-on-error", qualify)
        self.assertLess(steps.index(prepare), steps.index(qualify))
        self.assertLess(steps.index(qualify), steps.index(package))
        self.assertIn("--include-ignored", qualify["run"])
        self.assertIn("--ocio-runtime artifacts/ocio-native/prefix/ocio-runtime.json", package["run"])
        self.assertEqual(qualify["env"]["MOTION_TEST_OCIO_RUNTIME"],
                         "${{ github.workspace }}/artifacts/ocio-native/prefix/ocio-runtime.json")

    def test_managed_png_native_pixels_and_default_channels_gate_packaging(self):
        workflow = release_tests.yaml.load((desktop.ROOT / ".github/workflows/ci.yml").read_text(),
                                          Loader=release_tests.yaml.BaseLoader)
        steps = workflow["jobs"]["desktop"]["steps"]
        qualify = next(s for s in steps if s.get("name") == "Qualify native OCIO against independent references")
        package = next(s for s in steps if s.get("name") == "Package desktop release archive")
        self.assertNotIn("if", qualify)
        self.assertNotIn("continue-on-error", qualify)
        self.assertLess(steps.index(qualify), steps.index(package))
        commands = qualify["run"].splitlines()
        self.assertIn("cargo test -p motion-desktop-media --lib movie::frame::managed::tests --locked -- --ignored --nocapture --test-threads=1", commands)
        self.assertIn("cargo test -p motion-desktop --bin motion-studio engine::failure::color_preparation_tests::native_authored::png::managed_png_worker_preserves_default_channels_and_publishes_before_native_work --locked -- --ignored --exact --nocapture --test-threads=1", commands)
        self.assertEqual(qualify["env"]["MOTION_TEST_OCIO_RUNTIME"],
                         "${{ github.workspace }}/artifacts/ocio-native/prefix/ocio-runtime.json")

    def test_native_cubes_use_fresh_required_fixture_before_packaging(self):
        workflow = release_tests.yaml.load((desktop.ROOT / ".github/workflows/ci.yml").read_text(),
                                          Loader=release_tests.yaml.BaseLoader)
        steps = workflow["jobs"]["desktop"]["steps"]
        qualify = next(s for s in steps if s.get("name") == "Qualify native OCIO against independent references")
        package = next(s for s in steps if s.get("name") == "Package desktop release archive")
        self.assertNotIn("if", qualify)
        self.assertNotIn("continue-on-error", qualify)
        self.assertLess(steps.index(qualify), steps.index(package))
        self.assertIn("cargo test -p motion-render --test color_processor cubes::native_cube_buffers_match_cpu_for_linear_tetrahedral_and_mixed_tables --locked -- --ignored --exact --nocapture --test-threads=1", qualify["run"].splitlines())
        self.assertEqual(qualify["env"]["MOTION_TEST_OCIO_FAILURE_LIBRARY_DIR"],
                         "${{ github.workspace }}/artifacts/ocio-native/build/motion-ocio-bridge")

    def test_actual_managed_windows_worker_qualifies_before_packaging(self):
        workflow = release_tests.yaml.load((desktop.ROOT / ".github/workflows/ci.yml").read_text(),
                                          Loader=release_tests.yaml.BaseLoader)
        steps = workflow["jobs"]["desktop"]["steps"]
        native = next(s for s in steps if s.get("name") == "Qualify native OCIO against independent references")
        window = next(s for s in steps if s.get("name") == "Qualify managed Windows surface failure channels")
        package = next(s for s in steps if s.get("name") == "Package desktop release archive")
        self.assertEqual(window["if"], "runner.os == 'Windows'")
        self.assertNotIn("continue-on-error", window)
        self.assertLess(steps.index(native), steps.index(window))
        self.assertLess(steps.index(window), steps.index(package))
        self.assertIn("native_authored::window::managed_window_source_failure_preserves_project_and_default_error_channels", window["run"])
        self.assertIn("--ignored --exact", window["run"])
        self.assertEqual(window["env"]["MOTION_TEST_OCIO_RUNTIME"], native["env"]["MOTION_TEST_OCIO_RUNTIME"])
        self.assertEqual(window["env"]["MOTION_COLOR_TEST_EVIDENCE"], native["env"]["MOTION_COLOR_TEST_EVIDENCE"])


if __name__ == "__main__":
    unittest.main()
