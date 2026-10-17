"""Check the packages and publisher, including stale and incomplete inputs."""
import json
import os
from pathlib import Path
import plistlib
import stat
import struct
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import yaml

import ci_release
import package_desktop as desktop


class DesktopReleaseTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.folder = self.root / "artifacts/release"
        self.env = {"GITHUB_EVENT_NAME":"push", "GITHUB_REF": "refs/heads/main", "GITHUB_SHA": "a" * 40,
                    "GITHUB_RUN_ID": "123", "GITHUB_RUN_NUMBER": "7",
                    "GITHUB_REPOSITORY": "owner/project"}
        self.environment = patch.dict(os.environ, self.env)
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def binary(self, platform):
        data = bytearray(140)
        if platform == "windows-x86_64":
            data[:2] = b"MZ"
            struct.pack_into("<I", data, 60, 128)
            data[128:134] = b"PE\0\0\x64\x86"
        elif platform == "linux-x86_64":
            data[:6] = b"\x7fELF\x02\x01"
            struct.pack_into("<H", data, 18, 62)
        else:
            struct.pack_into("<II", data, 0, 0xFEEDFACF, 0x0100000C if platform == "macos-arm64" else 0x01000007)
        binary = self.root / (platform + ".bin")
        binary.write_bytes(data)
        return binary

    def pack(self, platform):
        binary = self.binary(platform)
        report = self.build_report(binary, platform)
        if platform.startswith("macos-"):
            with patch.object(desktop.sys, "platform", "darwin"), patch.object(desktop.subprocess, "run") as sign:
                info = desktop.package(binary, platform, report, self.folder)
                self.assertEqual(sign.call_count, 6)
                self.assertIn("--verify", sign.call_args.args[0])
                return info
        return desktop.package(binary, platform, report, self.folder)

    def build_report(self, binary, platform):
        prefix = self.root / (platform + "-runtime")
        prefix.mkdir(exist_ok=True)
        files = []
        for component in ("avcodec", "avformat", "avutil", "swscale"):
            name = (component + "-62.dll" if platform == "windows-x86_64" else
                    "lib" + component + (".62.dylib" if platform.startswith("macos-") else ".so.62"))
            library = prefix / name
            library.write_bytes(binary.read_bytes())
            files.append({"path": name, "sha256": desktop.digest(library)})
        license_file = prefix / "COPYING.LGPLv3"
        license_file.write_text("independently authored synthetic license fixture")
        manifest = prefix / "runtime.json"
        manifest.write_text(json.dumps({"schemaVersion": 1, "platform": platform,
            "author": "synthetic fixture", "modifications": "none",
            "provenance": {"url": "https://example.com/ffmpeg", "sha256": "b" * 64,
                           "revision": "test", "source": "https://example.com/source", "license": "LGPL-3.0-or-later"},
            "files": files, "licenses": [{"path": license_file.name, "sha256": desktop.digest(license_file)}]}))
        report = self.root / (platform + ".json")
        report.write_text(json.dumps({"task": "build", "release": True, "ffmpeg": True,
                                     "ffmpeg_runtime": str(manifest), "executable": str(binary)}))
        return report

    def all_assets(self):
        for platform in desktop.PLATFORMS:
            self.pack(platform)
        apk = self.folder / "MotionStudio-preview.apk"
        apk.write_bytes(b"already verified APK")
        info = {"commit": "a" * 40, "ref": self.env["GITHUB_REF"], "run": "123",
                "kind": "preview", "versionName": "0.1.0-preview.7", "versionCode": 7,
                "apk": apk.name, "sha256": desktop.digest(apk), "abis": ["arm64-v8a", "x86_64"]}
        (self.folder / "build-info.json").write_text(json.dumps(info))
        return info

    def test_architecture_is_checked_before_packaging(self):
        for platform in desktop.PLATFORMS:
            with self.subTest(platform=platform):
                binary = self.binary(platform)
                desktop.verify_binary(binary, platform)
                binary.write_bytes(b"not an executable")
                with self.assertRaises(ValueError):
                    desktop.verify_binary(binary, platform)
        with self.assertRaises(ValueError):
            desktop.verify_binary(self.binary("macos-arm64"), "macos-x86_64")

    def test_video_release_rejects_shell_builds_and_incomplete_or_corrupted_runtime(self):
        platform = "windows-x86_64"
        binary = self.binary(platform)
        report = self.build_report(binary, platform)
        build = json.loads(report.read_text())
        manifest_path = Path(build["ffmpeg_runtime"])
        original = json.loads(manifest_path.read_text())
        report.write_text(json.dumps({**build, "ffmpeg": False}))
        with self.assertRaisesRegex(ValueError, "FFmpeg-enabled"):
            desktop.package(binary, platform, report, self.folder)
        report.write_text(json.dumps(build))
        for description, change in (
            ("missing component", lambda m: m["files"].pop()),
            ("missing licenses", lambda m: m.update(licenses=[])),
            ("missing provenance", lambda m: m["provenance"].pop("source")),
            ("checksum mismatch", lambda m: m["files"][0].update(sha256="0" * 64)),
            ("traversal", lambda m: m["files"][0].update(path="../outside.dll")),
            ("wrong architecture", lambda m: m.update(platform="macos-arm64")),
        ):
            with self.subTest(case=description):
                changed = json.loads(json.dumps(original))
                change(changed)
                manifest_path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    desktop.package(binary, platform, report, self.folder)
                self.assertFalse(list(self.folder.glob("*.zip")))
        manifest_path.write_text(json.dumps(original))
        library = manifest_path.parent / original["files"][0]["path"]
        library.write_bytes(self.binary("macos-arm64").read_bytes())
        original["files"][0]["sha256"] = desktop.digest(library)
        manifest_path.write_text(json.dumps(original))
        with self.assertRaisesRegex(ValueError, "Windows"):
            desktop.package(binary, platform, report, self.folder)

    def test_feature_and_pull_request_builds_still_produce_validation_packages(self):
        for event, ref in [("push", "refs/heads/ci/desktop-preview-release"),
                           ("pull_request", "refs/pull/190/merge"),
                           ("workflow_dispatch", "refs/heads/main")]:
            with self.subTest(event=event,ref=ref), patch.dict(os.environ,GITHUB_EVENT_NAME=event,GITHUB_REF=ref):
                info = self.pack("windows-x86_64")
                self.assertEqual(info["kind"],"validation")
                self.assertIn("-validation-",info["archive"])

    def test_packages_contain_executables_licenses_and_macos_plists(self):
        for platform in desktop.PLATFORMS:
            with self.subTest(platform=platform):
                info = self.pack(platform)
                archive = self.folder / info["archive"]
                executable = desktop.PLATFORMS[platform][0]
                if platform == "linux-x86_64":
                    with tarfile.open(archive) as bundle:
                        self.assertEqual(bundle.getmember(executable).mode & 0o777, 0o755)
                        self.assertIn("README.txt", bundle.getnames())
                else:
                    with zipfile.ZipFile(archive) as bundle:
                        self.assertTrue(bundle.getinfo(executable).external_attr >> 16 & stat.S_IXUSR)
                        self.assertIn("LICENSE", bundle.namelist())
                        if platform.startswith("macos-"):
                            plist = plistlib.loads(bundle.read("MotionStudio.app/Contents/Info.plist"))
                            self.assertEqual(plist["CFBundleExecutable"], "motion-studio")
                            self.assertEqual(plist["CFBundleVersion"], "7")
                self.assertEqual(desktop.digest(archive), info["sha256"])
                self.assertTrue(info["capabilities"]["ffmpeg"])
                with (tarfile.open(archive) if platform == "linux-x86_64" else zipfile.ZipFile(archive)) as bundle:
                    names = bundle.getnames() if platform == "linux-x86_64" else bundle.namelist()
                    self.assertIn("FFmpeg-SOURCE.json", names)
                    self.assertIn("ATTRIBUTION.md", names)
                    self.assertIn("NOTICE", names)
                    self.assertTrue(any(n.startswith("licenses/ffmpeg/") for n in names))
                    self.assertTrue(all(e["path"] in names for e in info["runtimeFiles"]))

    def test_publisher_rechecks_the_runtime_inventory_inside_the_archive(self):
        release = self.all_assets()
        report = self.folder / "build-info-windows-x86_64.json"
        info = json.loads(report.read_text())
        info["runtimeFiles"][0]["sha256"] = "0" * 64
        report.write_text(json.dumps(info))
        with self.assertRaisesRegex(ValueError, "runtime checksum"):
            desktop.collect_assets(self.folder, release)

    def test_signing_failure_never_publishes_a_macos_archive(self):
        binary = self.binary("macos-arm64")
        report = self.build_report(binary, "macos-arm64")
        with patch.object(desktop.sys, "platform", "darwin"), patch.object(desktop.subprocess, "run", side_effect=subprocess.CalledProcessError(1,"codesign")):
            with self.assertRaises(subprocess.CalledProcessError):
                desktop.package(binary,"macos-arm64",report,self.folder)
        self.assertFalse(list(self.folder.glob("*.zip")))

    def test_all_packages_must_be_present_and_belong_to_the_same_run(self):
        info = self.all_assets()
        self.assertEqual(len(desktop.collect_assets(self.folder, info)), 5)
        report = self.folder / "build-info-macos-arm64.json"
        original = report.read_text()
        for field, value in [("commit","b"*40), ("ref","refs/heads/fix/work"), ("run","999"),
                             ("platform","macos-x86_64"), ("archive","../bad.zip"), ("signing","test-only-unsigned")]:
            with self.subTest(field=field):
                changed = json.loads(original); changed[field] = value
                report.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    desktop.collect_assets(self.folder, info)
        report.write_text(original)
        archive = self.folder / json.loads(original)["archive"]
        archive.write_bytes(b"corrupted archive")
        with self.assertRaises(ValueError):
            desktop.collect_assets(self.folder, info)
        archive.unlink()
        with self.assertRaises(FileNotFoundError):
            desktop.collect_assets(self.folder, info)

    def test_publisher_uploads_android_and_every_desktop_package_with_unified_hashes(self):
        info = self.all_assets()
        calls = []
        def gh(*args, **kwargs):
            calls.append(args)
            if "git/ref/heads/main" in " ".join(args):
                return subprocess.CompletedProcess(args,0,"a"*40+"\n","")
            if args[:2] == ("release", "view"):
                return subprocess.CompletedProcess(args,0,json.dumps({"isDraft":False,"url":"https://example.com/preview"}),"")
            return subprocess.CompletedProcess(args,0,"{}","")
        with patch.object(ci_release,"ROOT",self.root), patch.object(ci_release,"gh",side_effect=gh), patch.object(ci_release,"release_changelog",return_value="changes\n"):
            ci_release.publish()
        upload = next(call for call in calls if call[:2] == ("release","upload"))
        published = json.loads((self.folder / "build-info.json").read_text())
        self.assertEqual(published["schemaVersion"],2)
        self.assertEqual(len(published["artifacts"]),5)
        for item in published["artifacts"]:
            self.assertIn(str(self.folder/item["name"]),upload)
        self.assertEqual(len((self.folder/"SHA256SUMS").read_text().splitlines()),5)
        self.assertEqual(published["sha256"],info["sha256"])

    def test_stale_preview_and_incomplete_packages_never_write_to_github(self):
        self.all_assets()
        with patch.object(ci_release,"ROOT",self.root), patch.object(ci_release,"gh",return_value=subprocess.CompletedProcess([],0,"b"*40+"\n","")) as gh:
            ci_release.publish()
            self.assertEqual(gh.call_count,1)
            (self.folder/"build-info-linux-x86_64.json").unlink()
            gh.reset_mock()
            with self.assertRaises(FileNotFoundError):
                ci_release.publish()
            gh.assert_not_called()

    def test_desktop_only_publication_needs_no_apk_and_lists_only_four_packages(self):
        for platform in desktop.PLATFORMS:
            self.pack(platform)
        calls = []

        def gh(*args, **kwargs):
            calls.append(args)
            if "git/ref/heads/main" in " ".join(args):
                return subprocess.CompletedProcess(args, 0, "a" * 40 + "\n", "")
            if args[:2] == ("release", "view"):
                return subprocess.CompletedProcess(args, 0, json.dumps({"isDraft": False, "url": "https://example.com/preview"}), "")
            return subprocess.CompletedProcess(args, 0, "{}", "")

        with patch.object(ci_release, "ROOT", self.root), patch.object(ci_release, "gh", side_effect=gh), patch.object(ci_release, "release_changelog", return_value="changes\n"):
            ci_release.publish(desktop_only=True)
        info = json.loads((self.folder / "build-info.json").read_text(encoding="utf-8"))
        self.assertNotIn("apk", info)
        self.assertEqual({item["platform"] for item in info["artifacts"]}, set(desktop.PLATFORMS))
        upload = next(call for call in calls if call[:2] == ("release", "upload"))
        self.assertFalse(any(str(arg).endswith(".apk") for arg in upload))
        self.assertEqual(len((self.folder / "SHA256SUMS").read_text().splitlines()), 4)
        self.assertNotIn("Android", (self.folder / "release-notes.md").read_text(encoding="utf-8"))

    def test_desktop_only_rejects_missing_stale_corrupt_and_unsigned_inputs_before_gh(self):
        for platform in desktop.PLATFORMS:
            self.pack(platform)
        manifest = self.folder / "build-info-macos-arm64.json"
        original = manifest.read_text()
        for field, value in [("commit", "b" * 40), ("ref", "refs/heads/fix/work"),
                             ("run", "999"), ("versionName", "0.1.0-preview.6"),
                             ("kind", "release"), ("signing", "test-only-unsigned"),
                             ("sha256", "0" * 64), ("archive", "../bad.zip")]:
            with self.subTest(field=field):
                changed = json.loads(original)
                changed[field] = value
                manifest.write_text(json.dumps(changed))
                with patch.object(ci_release, "ROOT", self.root), patch.object(ci_release, "gh") as gh:
                    with self.assertRaises(ValueError):
                        ci_release.publish(desktop_only=True)
                    gh.assert_not_called()
        manifest.unlink()
        with patch.object(ci_release, "ROOT", self.root), patch.object(ci_release, "gh") as gh:
            with self.assertRaises(FileNotFoundError):
                ci_release.publish(desktop_only=True)
            gh.assert_not_called()

    def test_desktop_only_refuses_untrusted_events_even_with_main_or_tag_refs(self):
        for event, ref in [("pull_request", "refs/heads/main"),
                           ("workflow_dispatch", "refs/tags/v1.2.3"),
                           ("push", "refs/heads/feat/work")]:
            with self.subTest(event=event), patch.dict(os.environ, GITHUB_EVENT_NAME=event, GITHUB_REF=ref):
                with patch.object(ci_release, "ROOT", self.root), patch.object(ci_release, "gh") as gh:
                    with self.assertRaises(ValueError):
                        ci_release.publish(desktop_only=True)
                    gh.assert_not_called()

    def test_desktop_only_version_tags_preserve_stable_and_prerelease_publication(self):
        for version, prerelease in [("1.2.3", False), ("1.2.3-rc.1", True)]:
            with self.subTest(version=version), patch.dict(os.environ, GITHUB_REF="refs/tags/v" + version):
                for platform in desktop.PLATFORMS:
                    self.pack(platform)
                calls = []

                def gh(*args, **kwargs):
                    calls.append(args)
                    if args[:2] == ("release", "view"):
                        return subprocess.CompletedProcess(args, 1, "", "release not found")
                    return subprocess.CompletedProcess(args, 0, "{}", "")

                with patch.object(ci_release, "ROOT", self.root), patch.object(ci_release, "gh", side_effect=gh), patch.object(ci_release, "release_changelog", return_value="changes\n"):
                    ci_release.publish(desktop_only=True)
                info = json.loads((self.folder / "build-info.json").read_text())
                self.assertEqual(info["kind"], "release")
                self.assertEqual(info["versionName"], version)
                edit = next(call for call in calls if call[:2] == ("release", "edit"))
                self.assertEqual(edit[2], "v" + version)
                self.assertIn("--prerelease=" + str(prerelease).lower(), edit)
                self.assertIn("--latest=" + str(not prerelease).lower(), edit)

    def test_desktop_only_stale_preview_and_published_versions_are_never_replaced(self):
        for platform in desktop.PLATFORMS:
            self.pack(platform)
        with patch.object(ci_release, "ROOT", self.root), patch.object(ci_release, "gh", return_value=subprocess.CompletedProcess([], 0, "b" * 40 + "\n", "")) as gh:
            ci_release.publish(desktop_only=True)
            self.assertEqual(gh.call_count, 1)
        with patch.dict(os.environ, GITHUB_REF="refs/tags/v1.2.3"):
            for platform in desktop.PLATFORMS:
                self.pack(platform)
            calls = []

            def gh(*args, **kwargs):
                calls.append(args)
                if args[:2] == ("release", "view"):
                    return subprocess.CompletedProcess(args, 0, json.dumps({"isDraft": False, "url": "https://example.com/release"}), "")
                if args[:2] == ("release", "download"):
                    destination = Path(args[args.index("--dir") + 1])
                    (destination / "build-info.json").write_text(json.dumps({"commit": "b" * 40, "artifacts": []}))
                return subprocess.CompletedProcess(args, 0, "{}", "")

            with patch.object(ci_release, "ROOT", self.root), patch.object(ci_release, "gh", side_effect=gh):
                with self.assertRaisesRegex(RuntimeError, "cannot be replaced"):
                    ci_release.publish(desktop_only=True)
            self.assertFalse(any(call[:2] in (("release", "upload"), ("release", "edit")) for call in calls))

    def test_workflow_packages_every_platform_and_downloads_them_before_publication(self):
        workflow=yaml.load((desktop.ROOT/".github/workflows/ci.yml").read_text(),Loader=yaml.BaseLoader)
        matrix=workflow["jobs"]["desktop"]["strategy"]["matrix"]["include"]
        self.assertEqual({row["platform"] for row in matrix},set(desktop.PLATFORMS))
        for row in matrix:
            self.assertIn(row["platform"].split("-")[1], row["target"].replace("aarch64","arm64"))
        build_steps = workflow["jobs"]["desktop"]["steps"]
        build_index = next(i for i, s in enumerate(build_steps) if "build_desktop.py --task build" in s.get("run", ""))
        self.assertIn("--ffmpeg", build_steps[build_index]["run"])
        prepare_index = next(i for i, s in enumerate(build_steps) if "prepare_ffmpeg.py" in s.get("run", ""))
        verify_index = next(i for i, s in enumerate(build_steps) if "verify_desktop_video.py" in s.get("run", ""))
        upload_index = next(i for i, s in enumerate(build_steps) if s.get("with", {}).get("name", "").startswith("desktop-release-"))
        self.assertLess(prepare_index, build_index)
        self.assertLess(build_index, verify_index)
        self.assertLess(verify_index, upload_index)
        publish=workflow["jobs"]["publish"]
        self.assertIn("desktop",publish["needs"])
        steps=publish["steps"]
        download=next(step for step in steps if step.get("with",{}).get("pattern")=="desktop-release-*")
        self.assertEqual(download["with"]["merge-multiple"],"true")
        self.assertLess(steps.index(download), next(i for i,s in enumerate(steps) if s.get("run")=="python tools/ci_release.py publish-desktop"))


if __name__ == "__main__":
    unittest.main()
