"""Pin downloads before any third-party binary reaches a release prefix."""
import hashlib
import io
import json
from pathlib import Path
import shutil
import struct
import tempfile
import unittest
from unittest.mock import patch

import prepare_ffmpeg
import build_desktop


class FFmpegDownloadTest(unittest.TestCase):
    def test_release_link_flags_preserve_user_flags_and_platform_library_lookup(self):
        for platform in ("win32", "linux", "darwin"):
            with self.subTest(platform=platform):
                raw = r'-L native=C:\Native\Codec' if platform == "win32" else '-L "native=/path with spaces"'
                env = {"RUSTFLAGS": raw}
                with patch.object(build_desktop.sys, "argv", ["build_desktop.py", "--ffmpeg", "--task", "check"]), \
                     patch.object(build_desktop.sys, "platform", platform), \
                     patch.object(build_desktop, "ffmpeg_environment", return_value=dict(env)), \
                     patch.object(build_desktop, "run") as run:
                    self.assertEqual(build_desktop.main(), 0)
                actual = run.call_args.args[1]
                self.assertEqual(actual["RUSTFLAGS"], raw)
                if platform == "win32":
                    self.assertNotIn("CARGO_ENCODED_RUSTFLAGS", actual)
                else:
                    flags = actual["CARGO_ENCODED_RUSTFLAGS"].split("\x1f")
                    self.assertEqual(flags[:2], ["-L", "native=/path with spaces"])
                    self.assertIn("$ORIGIN/lib" if platform == "linux" else "@executable_path/../Frameworks", flags[-1])
                    if platform == "linux":
                        self.assertIn("--disable-new-dtags", flags[-1])

    def test_cached_input_is_reused_only_when_its_hash_matches(self):
        data = b"independently authored archive fixture"
        expected = hashlib.sha256(data).hexdigest()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "archive.zip"
            path.write_bytes(data)
            with patch.object(prepare_ffmpeg.urllib.request, "urlopen") as download:
                self.assertEqual(prepare_ffmpeg.download("https://example.com/archive", expected, path), path)
                download.assert_not_called()
            path.write_bytes(b"corrupt cached archive")
            with patch.object(prepare_ffmpeg.urllib.request, "urlopen", return_value=io.BytesIO(data)) as download:
                prepare_ffmpeg.download("https://example.com/archive", expected, path)
                download.assert_called_once()
                self.assertEqual(path.read_bytes(), data)

    def test_download_checksum_failure_does_not_publish_the_partial_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "archive.zip"
            with patch.object(prepare_ffmpeg.urllib.request, "urlopen", return_value=io.BytesIO(b"unexpected bytes")):
                with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                    prepare_ffmpeg.download("https://example.com/archive", "0" * 64, path)
            self.assertFalse(path.exists())


class FFmpegRuntimeCacheTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.output = Path(self.temporary.name) / "build"
        self.prefix = self.output / "prefix"
        self.prefix.mkdir(parents=True)
        data = bytearray(140)
        data[:2] = b"MZ"
        struct.pack_into("<I", data, 60, 128)
        data[128:134] = b"PE\0\0\x64\x86"
        files = []
        for component in ("avcodec", "avformat", "avutil", "swscale"):
            library = self.prefix / (component + "-62.dll")
            library.write_bytes(data)
            files.append({"path": library.name, "sha256": prepare_ffmpeg.digest(library)})
            header = self.prefix / "include" / ("lib" + component) / (component + ".h")
            header.parent.mkdir(parents=True)
            header.write_text("/* independently authored header fixture */")
        self.license = self.prefix / "LICENSE.txt"
        self.license.write_text("independently authored license fixture")
        self.pc = self.prefix / "lib" / "pkgconfig" / "libavcodec.pc"
        self.pc.parent.mkdir(parents=True)
        self.pc.write_text("prefix=" + str(self.prefix))
        self.manifest = self.prefix / "runtime.json"
        self.manifest.write_text(json.dumps({"schemaVersion": 1, "platform": "windows-x86_64",
            "author": "fixture author", "modifications": "none",
            "provenance": {"url": "https://example.com/fixture", "sha256": "b" * 64,
                           "source": "https://example.com/source", "revision": "fixture", "license": "MIT"},
            "files": files, "licenses": [{"path": self.license.name, "sha256": prepare_ffmpeg.digest(self.license)}]}))
        prepare_ffmpeg.seal_cache("windows-x86_64", self.prefix)

    def test_verified_runtime_is_reused_without_download_or_build(self):
        with patch.object(prepare_ffmpeg, "download") as download, patch.object(prepare_ffmpeg.subprocess, "run") as build:
            actual = prepare_ffmpeg.prepare("windows-x86_64", self.output, self.output / "downloads", reuse_verified=True)
            self.assertEqual(actual, self.prefix.resolve())
            download.assert_not_called()
            build.assert_not_called()

    def test_corrupted_libraries_headers_licenses_pkgconfig_and_manifest_are_rejected(self):
        paths = [self.prefix / "avcodec-62.dll", self.prefix / "include/libavcodec/avcodec.h", self.license, self.pc, self.manifest]
        for path in paths:
            with self.subTest(path=path.relative_to(self.prefix)):
                original = path.read_bytes()
                path.write_bytes(b"corrupt cache entry")
                self.assertFalse(prepare_ffmpeg.verified_cache("windows-x86_64", self.prefix))
                path.write_bytes(original)
                self.assertTrue(prepare_ffmpeg.verified_cache("windows-x86_64", self.prefix))
        self.license.unlink()
        self.assertFalse(prepare_ffmpeg.verified_cache("windows-x86_64", self.prefix))

    def test_wrong_platform_recipe_deployment_target_and_relocation_are_misses(self):
        self.assertFalse(prepare_ffmpeg.verified_cache("macos-arm64", self.prefix))
        with patch.object(prepare_ffmpeg, "cache_identity", return_value={"recipe": "changed"}):
            self.assertFalse(prepare_ffmpeg.verified_cache("windows-x86_64", self.prefix))
        with patch.dict(prepare_ffmpeg.os.environ, MACOSX_DEPLOYMENT_TARGET="different"):
            self.assertFalse(prepare_ffmpeg.verified_cache("windows-x86_64", self.prefix))
        relocated = self.output / "relocated"
        shutil.copytree(self.prefix, relocated)
        self.assertFalse(prepare_ffmpeg.verified_cache("windows-x86_64", relocated))

    def test_inventory_does_not_replace_required_runtime_and_header_validation(self):
        header = self.prefix / "include/libavcodec/avcodec.h"
        header.unlink()
        prepare_ffmpeg.seal_cache("windows-x86_64", self.prefix)
        self.assertFalse(prepare_ffmpeg.verified_cache("windows-x86_64", self.prefix))
        header.write_text("/* synthetic header */")
        manifest = json.loads(self.manifest.read_text())
        manifest["provenance"].pop("license")
        self.manifest.write_text(json.dumps(manifest))
        prepare_ffmpeg.seal_cache("windows-x86_64", self.prefix)
        self.assertFalse(prepare_ffmpeg.verified_cache("windows-x86_64", self.prefix))

    def test_invalid_cache_reenters_the_pinned_download_path(self):
        self.pc.write_text("corrupt prefix")
        with patch.object(prepare_ffmpeg, "download", side_effect=OSError("controlled offline download")) as download:
            with self.assertRaisesRegex(OSError, "controlled offline download"):
                prepare_ffmpeg.prepare("windows-x86_64", self.output, self.output / "downloads", reuse_verified=True)
        self.assertFalse(self.prefix.exists())
        self.assertEqual(download.call_args.args[:2], (prepare_ffmpeg.SOURCE_URL, prepare_ffmpeg.SOURCE_SHA256))

    def test_incomplete_and_missing_inventory_are_cache_misses(self):
        inventory = self.prefix / prepare_ffmpeg.CACHE_INVENTORY
        inventory.write_text("{")
        self.assertFalse(prepare_ffmpeg.verified_cache("windows-x86_64", self.prefix))
        inventory.unlink()
        self.assertFalse(prepare_ffmpeg.verified_cache("windows-x86_64", self.prefix))


if __name__ == "__main__":
    unittest.main()
