"""Pin downloads before any third-party binary reaches a release prefix."""
import hashlib
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import prepare_ffmpeg


class FFmpegDownloadTest(unittest.TestCase):
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
