"""Adversarial OCIO recipe checks; native numerical qualification is separate."""
import contextlib
import hashlib
import http.server
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

    def test_failed_cube_child_cannot_qualify_a_runtime_after_boundary_success(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build = ocio.Build(root, "windows-x86_64", "unused", "unused", 1)
            original = build.run
            phases = []
            def actual_child(component, phase, command):
                phases.append(phase)
                # Translate native executable paths to controlled real child
                # processes; this checks recipe publication, not native math.
                code = "import sys; sys.exit(7)" if phase == "cube-boundaries" else "pass"
                return original(component, phase, [sys.executable, "-c", code])
            stderr = io.StringIO()
            stdout = io.StringIO()
            with contextlib.redirect_stderr(stderr), contextlib.redirect_stdout(stdout):
                with patch.object(build, "run", side_effect=actual_child):
                    with self.assertRaises(subprocess.CalledProcessError) as failure:
                        build.qualify_bridge()
            self.assertEqual(failure.exception.returncode, 7)
            self.assertEqual(phases, ["exception-boundaries", "cube-boundaries"])
            self.assertFalse(build.bridge_qualified)
            self.assertFalse(json.loads((root / "in-flight.json").read_text())["running"])
            events = [json.loads(line) for line in (root / "events.jsonl").read_text().splitlines()]
            cube = [event for event in events if event["phase"] == "cube-boundaries"]
            self.assertEqual([(event["level"],event["operation"]) for event in cube],
                             [("info","start"),("error","done")])
            self.assertEqual(cube[-1]["exit_code"],7)
            self.assertIn("phase=cube-boundaries",stderr.getvalue())
            self.assertEqual(stdout.getvalue(),"")
            with self.assertRaisesRegex(ValueError,"exception boundary qualification is missing"):
                ocio.publish(build,[],{})
            self.assertFalse((root / "prefix/ocio-runtime.json").exists())


class OcioResourceQualificationTest(unittest.TestCase):
    def test_resource_child_failure_preserves_cause_and_prevents_manifest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build = ocio.Build(root, "windows-x86_64", "unused", "unused", 1)
            original = build.run
            phases = []
            def child(component, phase, command):
                phases.append(phase)
                code = "import sys; sys.stderr.write('controlled resource qualification cause'); sys.exit(9)" if phase == "resource-boundaries" else "pass"
                return original(component, phase, [sys.executable, "-c", code])
            stderr, stdout = io.StringIO(), io.StringIO()
            with contextlib.redirect_stderr(stderr), contextlib.redirect_stdout(stdout):
                with patch.object(build, "run", side_effect=child):
                    with self.assertRaises(subprocess.CalledProcessError) as failure:
                        build.qualify_bridge()
            self.assertEqual(failure.exception.returncode, 9)
            self.assertEqual(phases, ["exception-boundaries", "cube-boundaries", "resource-boundaries"])
            self.assertFalse(build.bridge_qualified)
            self.assertFalse(json.loads((root / "in-flight.json").read_text())["running"])
            events = [json.loads(line) for line in (root / "events.jsonl").read_text().splitlines()]
            resource = [event for event in events if event["phase"] == "resource-boundaries"]
            self.assertEqual([(event["level"], event["operation"]) for event in resource], [("info", "start"), ("error", "done")])
            self.assertEqual(resource[-1]["exit_code"], 9)
            self.assertIn("controlled resource qualification cause", (root / "logs/motion-ocio-bridge-resource-boundaries.stderr.txt").read_text())
            self.assertIn("phase=resource-boundaries", stderr.getvalue())
            self.assertEqual(stdout.getvalue(), "")
            with self.assertRaisesRegex(ValueError, "exception boundary qualification is missing"):
                ocio.publish(build, [], {})
            self.assertFalse((root / "prefix/ocio-runtime.json").exists())


class OcioSourceStageTest(unittest.TestCase):
    @staticmethod
    def events(root):
        return [json.loads(line) for line in (root / "events.jsonl").read_text().splitlines()]

    @staticmethod
    def stderr_events(text):
        return [json.loads(line.removeprefix("ERROR OCIO ")) for line in text.splitlines()
                if line.startswith("ERROR OCIO {")]

    @staticmethod
    def archive(name="root/payload.txt"):
        data = io.BytesIO()
        with tarfile.open(fileobj=data, mode="w:gz") as out:
            entry = tarfile.TarInfo(name)
            entry.size = len(b"independent source fixture")
            out.addfile(entry, io.BytesIO(b"independent source fixture"))
        return data.getvalue()

    @staticmethod
    @contextlib.contextmanager
    def source_server(data, entered=None, release=None):
        calls = []
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                calls.append(self.path)
                if entered is not None:
                    entered.set()
                if release is not None and not release.wait(10):
                    self.send_error(500, "test release missing")
                    return
                self.send_response(200)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            def log_message(self, *_):
                pass  # Suppress the test HTTP server's routine request banner.
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        worker = threading.Thread(target=server.serve_forever)
        worker.start()
        try:
            yield dict(name="owned-source", commit="a" * 40,
                       url=f"http://127.0.0.1:{server.server_port}/source.tar.gz",
                       bytes=len(data), sha256=hashlib.sha256(data).hexdigest()), calls
        finally:
            if release is not None:
                release.set()
            server.shutdown()
            server.server_close()
            worker.join(5)
            if worker.is_alive():
                raise RuntimeError("test source server did not stop")

    def test_actual_blocked_download_has_info_and_flight_before_response_then_recovers(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            entered, release = threading.Event(), threading.Event()
            with self.source_server(self.archive(), entered, release) as (item, calls):
                output = root / "build"
                output.mkdir()
                build = ocio.Build(output, "windows-x86_64", "unused", "unused", 1)
                errors, prepared = [], []
                def prepare():
                    try:
                        prepared.append(build.prepare_sources([item], root / "cache", False))
                    except BaseException as error:
                        errors.append(error)
                worker = threading.Thread(target=prepare)
                worker.start()
                try:
                    self.assertTrue(entered.wait(5), "real HTTP handler must be entered")
                    flight = json.loads((output / "in-flight.json").read_text())
                    self.assertTrue(flight["running"])
                    self.assertEqual(flight["phase"], "source_download")
                    self.assertEqual(flight["component"], item["name"])
                    self.assertEqual(self.events(output)[-1]["operation"], "start")
                    self.assertEqual(self.events(output)[-1]["level"], "info")
                    self.assertEqual(self.events(output)[-1]["phase"], "source_download")
                finally:
                    release.set()
                    worker.join(15)
                self.assertFalse(worker.is_alive())
                self.assertEqual(errors, [])
                self.assertEqual((prepared[0][item["name"]] / "payload.txt").read_bytes(), b"independent source fixture")
                self.assertFalse(json.loads((output / "in-flight.json").read_text())["running"])
                self.assertFalse(any(e["level"] == "error" for e in self.events(output)))
                # A verified cache hit is normal, including in offline mode.
                second = root / "offline-build"
                second.mkdir()
                cached = ocio.Build(second, "windows-x86_64", "unused", "unused", 1)
                cached.prepare_sources([item], root / "cache", True)
                self.assertEqual(calls, ["/source.tar.gz"])
                self.assertFalse(any(e["level"] == "error" for e in self.events(second)))

    def test_actual_download_and_extraction_failures_preserve_causes_and_output_channels(self):
        for extraction in (False, True):
            with self.subTest(extraction=extraction), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                with self.source_server(self.archive("../escape" if extraction else "root/payload.txt")) as (item, calls):
                    if not extraction:
                        item["sha256"] = "0" * 64
                    output = root / "build"
                    output.mkdir()
                    build = ocio.Build(output, "windows-x86_64", "unused", "unused", 1)
                    stdout, stderr = io.StringIO(), io.StringIO()
                    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                        with self.assertRaises(ValueError) as failure:
                            build.prepare_sources([item], root / "cache", False)
                    phase = "source_extract" if extraction else "source_download"
                    errors = [e for e in self.events(output) if e["level"] == "error"]
                    self.assertEqual(len(errors), 1)
                    self.assertEqual(errors[0]["phase"], phase)
                    self.assertEqual(errors[0]["component"], item["name"])
                    self.assertEqual(errors[0]["cause"], str(failure.exception))
                    self.assertEqual(errors[0]["cause_type"], "ValueError")
                    self.assertIn("elapsed_ms", errors[0])
                    self.assertIn(str(failure.exception), stderr.getvalue())
                    self.assertIn('"phase": "' + phase + '"', stderr.getvalue())
                    self.assertEqual(stdout.getvalue(), "")
                    self.assertFalse(json.loads((output / "in-flight.json").read_text())["running"])
                    self.assertFalse((root / "escape").exists())
                    self.assertFalse((output / "prefix/ocio-runtime.json").exists())
                    self.assertEqual(calls, ["/source.tar.gz"])
                    if not extraction:
                        self.assertEqual([path.suffix for path in (root / "cache").iterdir()], [".partial"])

    def test_actual_publish_refusal_is_a_separate_observed_stage(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build = ocio.Build(root, "windows-x86_64", "unused", "unused", 1)
            stdout, stderr = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                with self.assertRaisesRegex(ValueError, "exception boundary qualification is missing"):
                    with build.stage("runtime", "publish", build.prefix / "ocio-runtime.json"):
                        ocio.publish(build, [], {})
            events = self.events(root)
            self.assertEqual([(e["level"], e["operation"], e["phase"]) for e in events],
                             [("info", "start", "publish"), ("error", "done", "publish")])
            self.assertIn("qualification is missing", stderr.getvalue())
            self.assertEqual(stdout.getvalue(), "")
            self.assertFalse(json.loads((root / "in-flight.json").read_text())["running"])

    def test_actual_state_cleanup_failure_does_not_replace_the_original_io_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build = ocio.Build(root, "windows-x86_64", "unused", "unused", 1)
            original = []
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                with self.assertRaises(FileNotFoundError) as failure:
                    with build.stage("owned-source", "source_download", root / "source.tar.gz"):
                        state = root / "in-flight.json"
                        state.unlink()
                        state.mkdir()  # Real filesystem failure while clearing state.
                        try:
                            (root / "missing-source").read_bytes()
                        except FileNotFoundError as error:
                            original.append(error)
                            raise
            self.assertIs(failure.exception, original[0])
            events = self.events(root)
            self.assertEqual(events[1]["cause_type"], "FileNotFoundError")
            self.assertEqual(events[2]["operation"], "clear_in_flight_failed")
            self.assertEqual(self.stderr_events(stderr.getvalue())[0]["cause"], str(original[0]))
            self.assertIn("clear_in_flight_failed", stderr.getvalue())

    def test_actual_event_sink_failure_keeps_the_primary_exception_on_stderr(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build = ocio.Build(root, "windows-x86_64", "unused", "unused", 1)
            original = []
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                with self.assertRaises(FileNotFoundError) as failure:
                    with build.stage("owned-source", "source_extract", root / "sources"):
                        events = root / "events.jsonl"
                        events.unlink()
                        events.mkdir()  # Real log filesystem failure, no mocked logger.
                        try:
                            (root / "missing-archive").read_bytes()
                        except FileNotFoundError as error:
                            original.append(error)
                            raise
            self.assertIs(failure.exception, original[0])
            self.assertIn("ERROR OCIO observation", stderr.getvalue())
            self.assertEqual(self.stderr_events(stderr.getvalue())[0]["cause"], str(original[0]))
            self.assertFalse(json.loads((root / "in-flight.json").read_text())["running"])

    def test_cancellation_is_not_a_dependency_fault_and_nonzero_exit_is(self):
        for error in (KeyboardInterrupt(), SystemExit(0), SystemExit(7)):
            with self.subTest(error=repr(error)), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                build = ocio.Build(root, "windows-x86_64", "unused", "unused", 1)
                stderr = io.StringIO()
                with contextlib.redirect_stderr(stderr):
                    with self.assertRaises(type(error)) as failure:
                        with build.stage("owned-source", "source_download", root / "source.tar.gz"):
                            raise error
                self.assertIs(failure.exception, error)
                events = self.events(root)
                self.assertEqual(events[-1]["level"], "error" if isinstance(error, SystemExit) and error.code else "debug")
                self.assertEqual(bool(stderr.getvalue()), events[-1]["level"] == "error")
                self.assertFalse(json.loads((root / "in-flight.json").read_text())["running"])

    def test_cleanup_failure_after_success_is_reported_and_raised(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build = ocio.Build(root, "windows-x86_64", "unused", "unused", 1)
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                with self.assertRaises(OSError) as failure:
                    with build.stage("runtime", "publish", root / "prefix/ocio-runtime.json"):
                        state = root / "in-flight.json"
                        state.unlink()
                        state.mkdir()
            events = self.events(root)
            self.assertEqual(events[-1]["level"], "error")
            self.assertEqual(events[-1]["operation"], "clear_in_flight_failed")
            self.assertEqual(events[-1]["cause"], str(failure.exception))
            self.assertEqual(self.stderr_events(stderr.getvalue())[-1]["cause"], str(failure.exception))
            self.assertFalse(any(e["operation"] == "done" for e in events))

    def test_actual_closed_python_stderr_falls_back_to_raw_stderr_and_preserves_primary_error(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            script = "\n".join([
                "from pathlib import Path",
                "import sys",
                f"sys.path.insert(0, {str(Path(ocio.__file__).parent)!r})",
                "import prepare_ocio as ocio",
                "root = Path(sys.argv[1])",
                "build = ocio.Build(root, 'windows-x86_64', 'unused', 'unused', 1)",
                "sys.stderr = (root / 'closed-stderr').open('w')",
                "sys.stderr.close()",
                "try:",
                " with build.stage('owned-source', 'source_extract', root / 'sources'):",
                "  (root / 'original-missing-archive').read_bytes()",
                "except FileNotFoundError:",
                " sys.exit(37)",
                "sys.exit(99)",
            ])
            result = subprocess.run([sys.executable, "-c", script, str(root)],
                                    capture_output=True, text=True, encoding="utf-8", timeout=15)
            self.assertEqual(result.returncode, 37, result.stderr)
            self.assertEqual(result.stdout, "")
            self.assertIn("OCIO stderr stream unavailable type=ValueError", result.stderr)
            console = self.stderr_events(result.stderr)
            file = [e for e in self.events(root) if e["level"] == "error"]
            self.assertEqual(console, file)
            self.assertEqual(console[0]["cause_type"], "FileNotFoundError")
            self.assertIn("original-missing-archive", console[0]["cause"])
            self.assertFalse(json.loads((root / "in-flight.json").read_text())["running"])

    def test_stage_context_and_retained_cause_are_bounded_without_changing_the_exception(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build = ocio.Build(root, "windows-x86_64", "unused", "unused", 1)
            original = OSError("original prefix: " + "x" * 8192)
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                with self.assertRaises(OSError) as failure:
                    with build.stage("runtime", "publish", "p" * 8192):
                        raise original
            self.assertIs(failure.exception, original)
            event = self.events(root)[-1]
            self.assertEqual(event["cause"], str(original)[:2048])
            self.assertEqual(len(event["path"]), 1024)
            self.assertEqual(self.stderr_events(stderr.getvalue()), [event])
            self.assertGreater(len(str(failure.exception)), 8192)


if __name__ == "__main__":
    unittest.main()
