"""Guard desktop validation and publication routes without Android CI.

Run with: python -m unittest discover -s tools -p 'test_ci_*.py' -v
Requires the same pinned PyYAML dependency installed by the checks job.
"""
import ast
import fnmatch
import os
import ntpath
import posixpath
from pathlib import Path
import re
import subprocess
import sys
import tempfile
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
        if isinstance(node, ast.Compare) and len(node.ops) == 1:
            left, right = visit(node.left), visit(node.comparators[0])
            if isinstance(node.ops[0], ast.Eq):
                return left == right
            if isinstance(node.ops[0], ast.NotEq):
                return left != right
        if isinstance(node, ast.BoolOp):
            values = [bool(visit(value)) for value in node.values]
            if isinstance(node.op, ast.And):
                return all(values)
            if isinstance(node.op, ast.Or):
                return any(values)
        raise AssertionError("Unsupported CI gate: " + ast.dump(node))

    return visit(tree.body)


class WorkflowPolicyTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # BaseLoader keeps the YAML 1.1 word 'on' as a string and does not
        # reinterpret GitHub's string-valued outputs as Python booleans.
        cls.workflow = yaml.load(
            (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
            Loader=yaml.BaseLoader,
        )
        cls.checks = cls.workflow["jobs"]["checks"]

    def step(self, command):
        matches = [step for step in self.checks["steps"] if step.get("run") == command]
        self.assertEqual(len(matches), 1, command)
        return matches[0]

    def test_native_test_evidence_stays_under_upload_root_from_crate_cwd(self):
        desktop = self.workflow["jobs"]["desktop"]
        test = next(step for step in desktop["steps"] if step.get("name") == "Run desktop unit tests")
        upload = next(step for step in desktop["steps"] if step.get("with", {}).get("name") == "desktop-build-${{ matrix.platform }}")
        self.assertEqual(upload["if"], "always()")
        self.assertEqual(upload["with"]["path"], "artifacts/desktop/**")
        for paths, workspace in [(posixpath, "/runner/work/project/project"), (ntpath, r"D:\a\project\project")]:
            crate_cwd = paths.join(workspace, "desktop", "app")
            upload_root = paths.normpath(paths.join(workspace, "artifacts", "desktop"))
            for name in ["MOTION_SAMPLING_PLAYBACK_ARTIFACTS", "MOTION_FRAME_UI_ARTIFACTS"]:
                destination = test["env"][name].replace("${{ github.workspace }}", workspace)
                # Cargo runs the binary from desktop/app; a relative path used
                # to succeed locally yet silently miss the root upload glob.
                self.assertTrue(paths.isabs(destination), (name, destination))
                resolved = paths.normpath(paths.join(crate_cwd, destination))
                self.assertEqual(paths.commonpath([upload_root, resolved]), upload_root)

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

    def test_full_cross_platform_validation_remains_unconditional(self):
        validation = self.step("cargo test --workspace --exclude motion-android --locked --features motion-host/diagnostics -- --test-threads=1")
        self.assertNotIn("if", validation)
        self.assertEqual(set(self.workflow["jobs"]), {"checks", "desktop", "desktop-media", "publish"})
        desktop = self.workflow["jobs"]["desktop"]
        self.assertNotIn("if", desktop)
        self.assertEqual({entry["platform"] for entry in desktop["strategy"]["matrix"]["include"]},
                         {"linux-x86_64", "windows-x86_64", "macos-arm64", "macos-x86_64"})
        tests = next(step for step in desktop["steps"] if step.get("name") == "Run desktop unit tests")
        self.assertNotIn("if", tests)
        self.assertEqual(tests["run"], "cargo test -p motion-desktop -p motion-host -p motion-ui -p motion-desktop-media --locked -- --test-threads=1")
        text = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        for legacy in ("setup-java", "gradle/actions", "android-emulator-runner", "sdkmanager", "KEYSTORE", "motion-android/diagnostics", "tools/build_android.py", "tools/ci_android_ui.py", "tools/generate_editor_icons.py"):
            self.assertNotIn(legacy, text)

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
        self.assertEqual(publish["needs"], ["checks", "desktop", "desktop-media"])
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
