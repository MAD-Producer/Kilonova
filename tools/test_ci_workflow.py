"""Guard desktop validation and publication routes without Android CI.

Run with: python -m unittest discover -s tools -p 'test_ci_*.py' -v
Requires the same pinned PyYAML dependency installed by the checks job.
"""
import ast
import fnmatch
import json
import os
import ntpath
import posixpath
from pathlib import Path
import re
import subprocess
import shlex
import sys
import tempfile
import tomllib
import unittest

import yaml

from ci_release import ROOT, build_kind


CASES = [
    ("push", "refs/heads/main", "preview"),
    ("push", "refs/heads/feat/desktop", "validation"),
    ("push", "refs/heads/feat/nested/work", "validation"),
    ("push", "refs/heads/fix/ci-builds", "validation"),
    ("push", "refs/heads/refactor/editor", "validation"),
    ("push", "refs/heads/perf/preview", "validation"),
    ("push", "refs/heads/chore/tools", "validation"),
    ("push", "refs/tags/v1.2.3", "release"),
    ("push", "refs/tags/v1.2.3-rc.1", "release"),
    ("push", "refs/tags/v1", "validation"),
    ("push", "refs/tags/v01.2.3", "validation"),
    ("push", "refs/tags/v1.2.3;echo x", "validation"),
    ("push", "refs/tags/v1.2.3\n", "validation"),
    ("pull_request", "refs/pull/1/merge", "validation"),
    ("pull_request", "refs/heads/main", "validation"),
    ("workflow_dispatch", "refs/heads/main", "validation"),
    ("workflow_dispatch", "refs/heads/feat/desktop", "validation"),
    ("workflow_dispatch", "refs/tags/v1.2.3", "validation"),
]


def expression_value(expression, context):
    """Evaluate only the comparisons/boolean operators used by our CI gates.

    Unsupported syntax fails the guard test rather than silently enabling a
    release. Context values remain data; no workflow text is executed as Python.
    """
    if expression is None:
        return True  # Successful preceding steps are assumed in the route test.
    text = expression.strip().removeprefix("${{").removesuffix("}}").strip()
    tree = ast.parse(text.replace("&&", " and ").replace("||", " or "), mode="eval")

    def visit(node):
        if isinstance(node, ast.Constant):
            return node.value
        if isinstance(node, ast.Name):
            return context[node.id]
        if isinstance(node, ast.Attribute):
            return visit(node.value)[node.attr]
        if isinstance(node, ast.Subscript):
            return visit(node.value)[visit(node.slice)]
        if isinstance(node, ast.Compare) and len(node.ops) == 1:
            left, right = visit(node.left), visit(node.comparators[0])
            if isinstance(node.ops[0], ast.Eq):
                return left == right
            if isinstance(node.ops[0], ast.NotEq):
                return left != right
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == "fromJSON" and len(node.args) == 1 and not node.keywords:
            return json.loads(visit(node.args[0]))
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == "always" and not node.args and not node.keywords:
            return True
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == "failure" and not node.args and not node.keywords:
            return context.get("failure", False)
        if isinstance(node, ast.BoolOp):
            for value in node.values:
                result = visit(value)
                if isinstance(node.op, ast.And) and not result:
                    return result
                if isinstance(node.op, ast.Or) and result:
                    return result
            return result
        raise AssertionError("Unsupported CI gate: " + ast.dump(node))

    return visit(tree.body)


class UniqueKeyLoader(yaml.BaseLoader):
    """GitHub rejects duplicate mapping keys; PyYAML otherwise drops the first."""

    def construct_mapping(self, node, deep=False):
        mapping = {}
        for key_node, value_node in node.value:
            key = self.construct_object(key_node, deep=deep)
            if key in mapping:
                raise yaml.constructor.ConstructorError(
                    "while constructing a workflow mapping", node.start_mark,
                    f"duplicate mapping key: {key}", key_node.start_mark,
                )
            mapping[key] = self.construct_object(value_node, deep=deep)
        return mapping


class WorkflowPolicyTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # BaseLoader keeps the YAML 1.1 word 'on' as a string and does not
        # reinterpret GitHub's string-valued outputs as Python booleans.
        cls.workflow = yaml.load(
            (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
            Loader=UniqueKeyLoader,
        )
        cls.checks = cls.workflow["jobs"]["checks"]

    def test_yaml_loader_rejects_duplicate_job_environment_without_retyping_strings(self):
        with self.assertRaisesRegex(yaml.constructor.ConstructorError, "duplicate mapping key: env"):
            yaml.load("jobs:\n  desktop:\n    env: {FIRST: one}\n    env: {SECOND: two}\n", Loader=UniqueKeyLoader)
        parsed = yaml.load("on: {push: {}}\nenv: {FLAG: false}\n", Loader=UniqueKeyLoader)
        self.assertIn("on", parsed)
        self.assertEqual(parsed["env"]["FLAG"], "false")

    def step(self, command):
        matches = [step for step in self.checks["steps"] if step.get("run") == command]
        self.assertEqual(len(matches), 1, command)
        return matches[0]

    def test_native_test_evidence_stays_under_upload_root_from_crate_cwd(self):
        desktop = self.workflow["jobs"]["desktop"]
        test = next(step for step in desktop["steps"] if step.get("name") == "Run desktop unit tests")
        upload = next(step for step in desktop["steps"] if step.get("with", {}).get("name") == "desktop-build-${{ matrix.platform }}")
        self.assertEqual(upload["if"], "always() && github.event_name != 'pull_request'")
        self.assertEqual(upload["with"]["path"], "artifacts/desktop/**")
        for paths, workspace in [(posixpath, "/runner/work/project/project"), (ntpath, r"D:\a\project\project")]:
            crate_cwd = paths.join(workspace, "desktop", "app")
            upload_root = paths.normpath(paths.join(workspace, "artifacts", "desktop"))
            for name in ["MOTION_SAMPLING_PLAYBACK_ARTIFACTS", "MOTION_FRAME_UI_ARTIFACTS", "MOTION_POINT_SELECTION_ARTIFACTS"]:
                destination = test["env"][name].replace("${{ github.workspace }}", workspace)
                # Cargo runs the binary from desktop/app; a relative path used
                # to succeed locally yet silently miss the root upload glob.
                self.assertTrue(paths.isabs(destination), (name, destination))
                resolved = paths.normpath(paths.join(crate_cwd, destination))
                self.assertEqual(paths.commonpath([upload_root, resolved]), upload_root)

    def test_failed_pr_keeps_native_evidence_without_enabling_publication(self):
        steps = self.workflow["jobs"]["desktop"]["steps"]
        artifact_name = "desktop-failure-${{ matrix.platform }}"
        upload = next(s for s in steps if s.get("with", {}).get("name") == artifact_name)
        self.assertRegex(upload["uses"], r"actions/upload-artifact@[0-9a-f]{40}$")
        self.assertEqual(upload["with"]["retention-days"], "7")
        paths = upload["with"]["path"].splitlines()
        self.assertEqual(len(paths), 12)
        self.assertEqual(len(paths), len(set(paths)))
        self.assertIn("artifacts/desktop/sampling-playback/failure.json", paths)
        self.assertIn("artifacts/desktop/sampling-playback/stderr.log", paths)
        self.assertIn("artifacts/desktop/sampling-playback/motion-studio.log", paths)
        for path in paths:
            self.assertEqual(posixpath.dirname(path), "artifacts/desktop/sampling-playback")
            self.assertNotIn("*", path)
        for event in ["pull_request", "push", "workflow_dispatch"]:
            for failed in [False, True]:
                with self.subTest(event=event, failed=failed):
                    context = {"github": {"event_name": event}, "failure": failed}
                    self.assertEqual(expression_value(upload["if"], context),
                                     event == "pull_request" and failed)
                    if event == "pull_request":
                        for step in steps:
                            if step.get("with", {}).get("name", "").startswith("desktop-release-"):
                                self.assertFalse(expression_value(step["if"], context))

    def test_pull_requests_replace_feature_pushes_including_stacked_bases(self):
        triggers = self.workflow["on"]
        self.assertEqual(triggers["push"]["branches"], ["main"])
        self.assertEqual(triggers["push"]["tags"], ["v*"])
        self.assertFalse(triggers["pull_request"])
        self.assertIn("workflow_dispatch", triggers)

        def triggered(event, ref):
            route = triggers.get(event)
            if event not in triggers:
                return False
            if not route:
                return True
            prefix = "refs/tags/" if ref.startswith("refs/tags/") else "refs/heads/"
            patterns = route.get("tags" if prefix == "refs/tags/" else "branches", ["**"])
            return any(fnmatch.fnmatchcase(ref.removeprefix(prefix), pattern) for pattern in patterns)

        for base in ["main", "fix/native-ci-diagnostics", "feat/editor-preferences", "feat/custom-shortcuts", "feat/layer-navigation"]:
            with self.subTest(base=base):
                self.assertTrue(triggered("pull_request", "refs/heads/" + base))
        for head in ["feat/desktop", "feat/nested/work", "fix/native-ci-diagnostics", "ci/pr-validation-deduplication"]:
            with self.subTest(head=head):
                self.assertFalse(triggered("push", "refs/heads/" + head))
                self.assertTrue(triggered("workflow_dispatch", "refs/heads/" + head))
        self.assertTrue(triggered("push", "refs/heads/main"))
        self.assertTrue(triggered("push", "refs/tags/v1.2.3"))
        self.assertFalse(triggered("push", "refs/tags/experimental"))

    def test_concurrency_cancels_superseded_validation_without_crossing_publication(self):
        concurrency = self.workflow["concurrency"]

        def route(event, ref, head="", workflow="CI", run_id="1"):
            context = {"github": {"event_name": event, "ref": ref, "head_ref": head,
                                  "workflow": workflow, "run_id": run_id}}
            group = re.sub(r"\$\{\{(.*?)\}\}",
                           lambda match: str(expression_value(match.group(), context)),
                           concurrency["group"])
            return group.lower(), expression_value(concurrency["cancel-in-progress"], context)

        first = route("pull_request", "refs/pull/373/merge", "feat/camera-lens-settings")
        newer = route("pull_request", "refs/pull/373/merge", "feat/camera-lens-settings", run_id="2")
        self.assertEqual(first, newer)
        self.assertTrue(newer[1])
        fork = route("pull_request", "refs/pull/374/merge", "feat/camera-lens-settings")
        self.assertNotEqual(first[0], fork[0])
        self.assertNotEqual(first[0], route("pull_request", "refs/pull/373/merge", workflow="Other")[0])
        for ref in ["refs/heads/main", "refs/tags/v1.2.3"]:
            with self.subTest(ref=ref):
                publication = route("push", ref)
                manual = route("workflow_dispatch", ref)
                self.assertNotEqual(publication[0], manual[0])
                self.assertFalse(publication[1])
                self.assertTrue(manual[1])
                # A fork branch named main or v1.2.3 must not cancel a release.
                for head in ["main", "v1.2.3"]:
                    self.assertNotEqual(publication[0], route("pull_request", "refs/pull/375/merge", head)[0])
        self.assertNotEqual(route("push", "refs/heads/main")[0], route("push", "refs/tags/v1.2.3")[0])

    def test_dependency_audit_runs_for_prs_and_gates_publication(self):
        job = self.workflow["jobs"]["dependency-audit"]
        self.assertNotIn("if", job)
        self.assertNotIn("continue-on-error", job)
        commands = [step["run"] for step in job["steps"] if "run" in step]
        self.assertEqual(commands, [
            "git -c http.version=HTTP/1.1 clone --depth 1 https://github.com/RustSec/advisory-db.git artifacts/dependency-audit-db",
            "python tools/ci_dependency_audit.py --database artifacts/dependency-audit-db",
        ])
        for step in job["steps"]:
            self.assertNotIn("continue-on-error", step)
        uploads = [step for step in job["steps"] if step.get("uses", "").startswith("actions/upload-artifact@")]
        self.assertEqual(len(uploads), 1)
        self.assertEqual(uploads[0]["if"], "always()")
        self.assertEqual(set(uploads[0]["with"]["path"].splitlines()), {
            "artifacts/dependency-audit/audit.json",
            "artifacts/dependency-audit/audit.stderr.log",
            "artifacts/dependency-audit/receipt.json",
        })
        self.assertEqual(uploads[0]["with"]["retention-days"], "7")
        self.assertIn("dependency-audit", self.workflow["jobs"]["publish"]["needs"])

    def test_native_and_workspace_jobs_partition_all_supported_packages(self):
        validation = next(step for step in self.checks["steps"] if step.get("name") == "Run Rust and software Vulkan tests")
        self.assertNotIn("if", validation)
        self.assertEqual(set(self.workflow["jobs"]), {"checks", "desktop", "desktop-media", "dependency-audit", "publish"})
        desktop = self.workflow["jobs"]["desktop"]
        self.assertNotIn("if", desktop)
        tests = next(step for step in desktop["steps"] if step.get("name") == "Run desktop unit tests")
        self.assertNotIn("if", tests)
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]
        members = {tomllib.loads((ROOT / member / "Cargo.toml").read_text(encoding="utf-8"))["package"]["name"] for member in workspace["members"]}
        core_args, native_args = shlex.split(validation["run"]), shlex.split(tests["run"])
        excluded = {core_args[i + 1] for i, arg in enumerate(core_args) if arg == "--exclude"}
        native = {native_args[i + 1] for i, arg in enumerate(native_args) if arg == "-p"}
        self.assertIn("--workspace", core_args)
        self.assertEqual(excluded, native | {"motion-android"})
        self.assertEqual((members - excluded) | native, members - {"motion-android"})
        native_features = native_args[native_args.index("--features") + 1].split(",")
        self.assertIn("motion-host/diagnostics", native_features)
        self.assertIn("motion-host/ai-depth", native_features)
        self.assertIn("motion-ui/gpu-tests", native_features)
        manifests = [tomllib.loads((ROOT / member / "Cargo.toml").read_text(encoding="utf-8")) for member in workspace["members"]]
        gpu_packages = {manifest["package"]["name"] for manifest in manifests if "gpu-tests" in manifest.get("features", {})}
        core_features = set(core_args[core_args.index("--features") + 1].split(","))
        self.assertLessEqual({f"{package}/gpu-tests" for package in gpu_packages - excluded}, core_features)
        self.assertLessEqual({f"{package}/gpu-tests" for package in gpu_packages & native}, set(native_features))
        for args in [core_args, native_args]:
            self.assertIn("--locked", args)
            self.assertIn("--test-threads=1", args)
            self.assertNotIn("--skip", args)
        text = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        for legacy in ("setup-java", "gradle/actions", "android-emulator-runner", "sdkmanager", "KEYSTORE", "motion-android/diagnostics", "tools/build_android.py", "tools/ci_android_ui.py", "tools/generate_editor_icons.py"):
            self.assertNotIn(legacy, text)

    def test_frozen_depth_exports_are_enabled_and_evidence_has_upload_routes(self):
        desktop = self.workflow["jobs"]["desktop"]
        native = next(s for s in desktop["steps"] if s.get("name") == "Run desktop unit tests")
        args = shlex.split(native["run"])
        self.assertIn("motion-desktop-media/ai-depth", args[args.index("--features") + 1].split(","))
        self.assertNotIn("ffmpeg", args[args.index("--features") + 1].split(","))
        failed = next(s for s in desktop["steps"] if s.get("name") == "Preserve failed PR frozen export evidence")
        self.assertEqual(failed["if"], "failure() && github.event_name == 'pull_request'")
        media = self.workflow["jobs"]["desktop-media"]
        test = next(s for s in media["steps"] if s.get("name") == "Test libav probing, seeks and YUV frames")
        args = shlex.split(test["run"])
        self.assertLessEqual({"ffmpeg", "gpu-tests", "ai-depth"}, set(args[args.index("--features") + 1].split(",")))
        upload = next(s for s in media["steps"] if s.get("name") == "Preserve frozen movie validation evidence")
        self.assertEqual(upload["if"], "always()")
        self.assertEqual(upload["with"]["path"], "artifacts/desktop-media/**")
        for step, prefix in [(native, "artifacts/desktop/"), (test, "artifacts/desktop-media/")]:
            for name in ["MOTION_FROZEN_DEPTH_ARTIFACTS", "MOTION_IMPORTED_CAMERA_OUTPUT_ARTIFACTS"]:
                self.assertTrue(step["env"][name].startswith("${{ github.workspace }}/" + prefix))
        for path in ["artifacts/desktop/frozen-depth/**", "artifacts/desktop/imported-camera/**"]:
            self.assertIn(path, failed["with"]["path"].splitlines())

    def test_cpu_only_feature_is_not_reenabled_by_internal_dependencies(self):
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]
        manifests = [tomllib.loads((ROOT / member / "Cargo.toml").read_text(encoding="utf-8")) for member in workspace["members"]]
        gpu_packages = {manifest["package"]["name"] for manifest in manifests if "gpu-tests" in manifest.get("features", {})}
        self.assertTrue(gpu_packages)
        for manifest in manifests:
            name = manifest["package"]["name"]
            if name in gpu_packages:
                self.assertIn("gpu-tests", manifest["features"]["default"], name)
            scopes = [manifest, *manifest.get("target", {}).values()]
            for scope in scopes:
                for kind in ("dependencies", "dev-dependencies", "build-dependencies"):
                    for dependency, options in scope.get(kind, {}).items():
                        if dependency in gpu_packages:
                            self.assertIs(options.get("default-features"), False, f"{name}: {kind}.{dependency}")

    def test_prs_run_three_native_platforms_without_release_steps(self):
        desktop = self.workflow["jobs"]["desktop"]
        matrix = desktop["strategy"]["matrix"]["include"]
        release_names = {
            "Install portable libclang for FFmpeg bindings", "Prepare pinned shared LGPL FFmpeg runtime",
            "Release desktop binary with video support", "Verify packaged-runtime video decoding",
            "Package desktop release archive", "Verify video decoding from the extracted release archive",
            "Build standalone UI framework gallery", "Verify native windows, language and shared MCP",
            "Verify independent UI framework",
        }
        for event in ["pull_request", "push", "workflow_dispatch"]:
            context = {"github": {"event_name": event}, "runner": {"os": "Linux"},
                       "steps": {name: {"outputs": {"cache-hit": "false"}} for name in ["libclang_cache", "ffmpeg_cache"]}}
            rows = expression_value(matrix, context)
            expected = {"linux-x86_64", "windows-x86_64", "macos-x86_64"}
            if event != "pull_request":
                expected.add("macos-arm64")
            self.assertEqual({row["platform"] for row in rows}, expected)
            self.assertEqual(len(rows), len(expected))
            active = [s for s in desktop["steps"] if expression_value(s.get("if"), context)]
            active_names = {s.get("name") for s in active}
            self.assertIn("Run desktop unit tests", active_names)
            self.assertIn("Build the desktop shell", active_names)
            self.assertEqual(active_names & release_names, set() if event == "pull_request" else release_names)
            if event == "pull_request":
                self.assertFalse(any(s.get("uses", "").startswith("actions/upload-artifact@") for s in active))
                self.assertFalse(any("--ffmpeg" in s.get("run", "") or "prepare_ffmpeg.py" in s.get("run", "") for s in active))
            self.assertEqual(expression_value(self.workflow["jobs"]["desktop-media"]["if"], context), event != "pull_request")
        for runner, expected in [("Linux", "ubuntu-24.04"), ("Windows", "windows-2022"), ("macOS", "macos-15-intel")]:
            rows = expression_value(matrix, {"github": {"event_name": "pull_request"}})
            self.assertIn(expected, {row["os"] for row in rows}, runner)

    def test_runtime_caches_are_pinned_partitioned_and_never_skip_verification(self):
        steps = self.workflow["jobs"]["desktop"]["steps"]
        for identity, required in [("libclang", ["runner.os", "runner.arch", "18.1.1"]),
                                   ("ffmpeg", ["runner.os", "matrix.platform", "hashFiles('tools/prepare_ffmpeg.py')"])]:
            restore = next(s for s in steps if s.get("id") == identity + "_cache")
            save = next(s for s in steps if s.get("uses", "").startswith("actions/cache/save@") and s["with"]["key"] == restore["with"]["key"])
            self.assertRegex(restore["uses"], r"actions/cache/restore@[0-9a-f]{40}$")
            self.assertRegex(save["uses"], r"actions/cache/save@[0-9a-f]{40}$")
            self.assertNotIn("restore-keys", restore["with"])
            for token in required:
                self.assertIn(token, restore["with"]["key"])
            for event, hit, expected in [("pull_request", "false", False), ("workflow_dispatch", "false", False),
                                         ("push", "false", True), ("push", "true", False)]:
                context = {"github": {"event_name": event}, "steps": {identity + "_cache": {"outputs": {"cache-hit": hit}}}}
                self.assertEqual(expression_value(save["if"], context), expected)
        prepare = next(s for s in steps if s.get("name") == "Prepare pinned shared LGPL FFmpeg runtime")
        self.assertIn("--reuse-verified", prepare["run"])
        self.assertNotIn("cache-hit", prepare["if"])
        for name in ["Verify packaged-runtime video decoding", "Verify video decoding from the extracted release archive"]:
            step = next(s for s in steps if s.get("name") == name)
            self.assertEqual(step["if"], "github.event_name != 'pull_request'")

    def test_policy_cli_runs_without_secrets_and_writes_step_outputs(self):
        script = ROOT / "tools/ci_release.py"
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "outputs"
            for event, ref, expected in CASES:
                with self.subTest(event=event, ref=ref):
                    self.assertEqual(build_kind(event, ref), expected)
                    output.write_text("existing=output\n", encoding="utf-8")
                    env = {
                        name: value for name, value in os.environ.items()
                        if not name.startswith(("PREVIEW_", "RELEASE_", "MOTION_KEY"))
                    }
                    env.update(GITHUB_EVENT_NAME=event, GITHUB_REF=ref, GITHUB_OUTPUT=str(output))
                    subprocess.run(
                        [sys.executable, str(script), "policy"],
                        env=env, check=True, capture_output=True, text=True,
                    )
                    publish = str(expected != "validation").lower()
                    self.assertEqual(
                        output.read_text(encoding="utf-8"),
                        f"existing=output\nkind={expected}\npublish={publish}\n",
                    )

    def test_desktop_publication_follows_policy_without_android_dependencies(self):
        policy = self.step("python tools/ci_release.py policy")
        self.assertEqual(policy["id"], "release_policy")
        self.assertNotIn("if", policy)
        publish = self.workflow["jobs"]["publish"]
        self.assertEqual(publish["needs"], ["checks", "desktop", "desktop-media", "dependency-audit"])
        self.assertEqual(publish["permissions"], {"contents": "write"})
        self.assertIn("python tools/ci_release.py publish-desktop", [step.get("run") for step in publish["steps"]])
        for event, ref, kind in CASES:
            with self.subTest(event=event, ref=ref):
                context = {
                    "github": {"event_name": event, "ref": ref},
                    "steps": {"release_policy": {"outputs": {
                        "kind": kind, "publish": str(kind != "validation").lower(),
                    }}},
                }
                job_output = expression_value(self.checks["outputs"]["publish"], context)
                context["needs"] = {"checks": {"outputs": {"publish": job_output}}}
                self.assertEqual(expression_value(publish.get("if"), context), kind != "validation")

    def test_missing_policy_outputs_do_not_enable_publish(self):
        context = {
            "steps": {"release_policy": {"outputs": {"kind": "", "publish": ""}}},
            "needs": {"checks": {"outputs": {"publish": ""}}},
        }
        self.assertFalse(expression_value(self.workflow["jobs"]["publish"]["if"], context))


if __name__ == "__main__":
    unittest.main()
