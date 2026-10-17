"""Pin downloads before any third-party binary reaches a release prefix."""
import hashlib
import io
from pathlib import Path
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


if __name__ == "__main__":
    unittest.main()
