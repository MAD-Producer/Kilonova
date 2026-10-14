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
        report = self.root / (platform + ".json")
        report.write_text(json.dumps({"task": "build", "release": True, "ffmpeg": False, "executable": str(binary)}))
        if platform.startswith("macos-"):
            with patch.object(desktop.sys, "platform", "darwin"), patch.object(desktop.subprocess, "run") as sign:
                info = desktop.package(binary, platform, report, self.folder)
                self.assertEqual(sign.call_count, 2)
                self.assertIn("--verify", sign.call_args.args[0])
                return info
        return desktop.package(binary, platform, report, self.folder)

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

    def test_signing_failure_never_publishes_a_macos_archive(self):
        binary = self.binary("macos-arm64")
        report = self.root / "build.json"
        report.write_text(json.dumps({"task":"build", "release":True,"ffmpeg":False,"executable":str(binary)}))
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

    def test_workflow_packages_every_platform_and_downloads_them_before_publication(self):
        workflow=yaml.load((desktop.ROOT/".github/workflows/ci.yml").read_text(),Loader=yaml.BaseLoader)
        matrix=workflow["jobs"]["desktop"]["strategy"]["matrix"]["include"]
        self.assertEqual({row["platform"] for row in matrix},set(desktop.PLATFORMS))
        for row in matrix:
            self.assertIn(row["platform"].split("-")[1], row["target"].replace("aarch64","arm64"))
        publish=workflow["jobs"]["publish"]
        self.assertIn("desktop",publish["needs"])
        steps=publish["steps"]
        download=next(step for step in steps if step.get("with",{}).get("pattern")=="desktop-release-*")
        self.assertEqual(download["with"]["merge-multiple"],"true")
        self.assertLess(steps.index(download), next(i for i,s in enumerate(steps) if s.get("run")=="python tools/ci_release.py publish"))


if __name__ == "__main__":
    unittest.main()
