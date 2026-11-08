"""Adversarial OCIO recipe checks; native numerical qualification is separate."""
import contextlib
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

import prepare_ocio as ocio


class OcioRecipeTest(unittest.TestCase):
    def test_lock_has_exact_source_and_license_identities_for_every_linked_dependency(self):
        components = ocio.sources()
        self.assertEqual({i["name"] for i in components}, {"OpenColorIO", "expat", "yaml-cpp", "pystring", "Imath", "zlib", "minizip-ng", "sse2neon"})
        for item in components:
            self.assertRegex(item["commit"], r"^[0-9a-f]{40}$")
            self.assertEqual(item["url"], f"https://codeload.github.com/{item['repository']}/tar.gz/{item['commit']}")
            self.assertRegex(item["sha256"], r"^[0-9a-f]{64}$")
            self.assertRegex(item["license_sha256"], r"^[0-9a-f]{64}$")
            self.assertTrue(item["author"] and item["license"] and item["modifications"])

    def test_apple_arm_uses_the_upstream_pinned_header_without_ambient_fetching(self):
        arm = ocio.sources("macos-arm64")
        self.assertEqual(len(arm), 8)
        self.assertEqual([item["name"] for item in arm if item.get("platforms")], ["sse2neon"])
        for platform in ("windows-x86_64", "linux-x86_64", "macos-x86_64"):
            self.assertEqual(len(ocio.sources(platform)), 7)
            self.assertNotIn("sse2neon", [item["name"] for item in ocio.sources(platform)])

    def test_corrupt_offline_cache_is_rejected_without_network_or_replacement(self):
        with tempfile.TemporaryDirectory() as temporary:
            cache = Path(temporary)
            item = ocio.sources()[0]
            path = cache / (item["name"] + "-" + item["commit"] + ".tar.gz")
            path.write_bytes(b"corrupt retained original")
            with patch.object(ocio.urllib.request, "urlopen") as network:
                with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                    ocio.download(item, cache, True)
                self.assertEqual(path.read_bytes(), b"corrupt retained original")
                network.assert_not_called()
            path.unlink()
            with self.assertRaisesRegex(FileNotFoundError, "offline source missing"):
                ocio.download(item, cache, True)

    def test_bad_download_never_publishes_a_source_archive(self):
        with tempfile.TemporaryDirectory() as temporary:
            cache = Path(temporary)
            with patch.object(ocio.urllib.request, "urlopen", return_value=io.BytesIO(b"wrong source")):
                with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                    ocio.download(ocio.sources()[0], cache, False)
            self.assertEqual([p.suffix for p in cache.iterdir()], [".partial"])

    def test_archive_traversal_and_external_links_are_rejected_before_extraction(self):
        for name, link in (("../escape", None), ("root/symlink", "../../escape"), ("root/C:/escape", None)):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive = root / "source.tar.gz"
                with tarfile.open(archive, "w:gz") as out:
                    entry = tarfile.TarInfo(name)
                    if link:
                        entry.type, entry.linkname = tarfile.SYMTYPE, link
                    out.addfile(entry)
                with self.assertRaises((ValueError, tarfile.FilterError)):
                    ocio.extract(archive, root / "sources")
                self.assertFalse((root / "escape").exists())

    def test_actual_native_exit_keeps_cause_logs_stderr_and_stdout_isolation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build = ocio.Build(root, "windows-x86_64", "unused", "unused", 1)
            stdout, stderr = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                with self.assertRaises(subprocess.CalledProcessError) as failure:
                    build.run("control", "failure", [sys.executable, "-c", "import sys; print('protocol-output'); print('original-native-cause',file=sys.stderr); sys.exit(7)"])
            self.assertEqual(failure.exception.returncode, 7)
            self.assertEqual(stdout.getvalue(), "")
            self.assertIn("exit=7", stderr.getvalue())
            self.assertIn("original-native-cause", (root / "logs/control-failure.stderr.txt").read_text())
            self.assertEqual((root / "logs/control-failure.stdout.txt").read_text().strip(), "protocol-output")
            events = [json.loads(line) for line in (root / "events.jsonl").read_text().splitlines()]
            self.assertEqual([(e["level"], e["operation"]) for e in events], [("info", "start"), ("error", "done")])
            self.assertFalse(json.loads((root / "in-flight.json").read_text())["running"])
            self.assertFalse((root / "prefix/ocio-runtime.json").exists())

    def test_controlled_block_publishes_info_and_flight_before_child_handler(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build = ocio.Build(root, "windows-x86_64", "unused", "unused", 1)
            entered, release = root / "entered", root / "release"
            script = root / "blocked.py"
            script.write_text("from pathlib import Path\nimport sys,time\nentered,release=map(Path,sys.argv[1:])\nentered.touch()\nend=time.monotonic()+10\nwhile not release.exists():\n if time.monotonic()>end: raise RuntimeError('control release missing')\n time.sleep(.01)\n")
            errors = []
            def child():
                try:
                    build.run("control", "blocked", [sys.executable, str(script), str(entered), str(release)])
                except BaseException as error:
                    errors.append(error)
            worker = threading.Thread(target=child)
            worker.start()
            try:
                deadline = time.monotonic() + 5
                while not entered.exists() and time.monotonic() < deadline:
                    time.sleep(.01)
                self.assertTrue(entered.exists(), "actual child must enter the controlled handler")
                self.assertTrue(json.loads((root / "in-flight.json").read_text())["running"])
                events = [json.loads(line) for line in (root / "events.jsonl").read_text().splitlines()]
                self.assertEqual([(e["level"],e["operation"]) for e in events], [("info","start")])
            finally:
                release.touch()
                worker.join(15)
            self.assertFalse(worker.is_alive())
            self.assertEqual(errors, [])
            self.assertFalse(json.loads((root / "in-flight.json").read_text())["running"])

    def test_recipe_mutation_prevents_manifest_publication(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build = ocio.Build(root, "windows-x86_64", "unused", "unused", 1)
            source = root / "own-source"
            source.write_bytes(b"before build")
            build.identity = {str(source): hashlib.sha256(source.read_bytes()).hexdigest()}
            source.write_bytes(b"changed during build")
            with self.assertRaisesRegex(ValueError, "changed during build"):
                ocio.publish(build, [], {})
            self.assertFalse((root / "prefix/ocio-runtime.json").exists())

    def test_missing_exception_qualification_prevents_manifest_publication(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build = ocio.Build(root, "windows-x86_64", "unused", "unused", 1)
            with self.assertRaisesRegex(ValueError, "exception boundary qualification is missing"):
                ocio.publish(build, [], {})
            self.assertFalse((root / "prefix/ocio-runtime.json").exists())


if __name__ == "__main__":
    unittest.main()
