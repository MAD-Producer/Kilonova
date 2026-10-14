"""Guard the actual workflow's validation, signing and publication routes.

Run with: python -m unittest discover -s tools -p 'test_ci_*.py' -v
Requires the same pinned PyYAML dependency installed by the checks job.
"""
import ast
import os
from pathlib import Path
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
        cls.android = cls.workflow["jobs"]["android"]

    def step(self, command):
        matches = [step for step in self.android["steps"] if step.get("run") == command]
        self.assertEqual(len(matches), 1, command)
        return matches[0]

    def test_all_branch_pushes_and_main_pull_requests_run_validation(self):
        triggers = self.workflow["on"]
        self.assertEqual(triggers["push"]["branches"], ["**"])
        self.assertEqual(triggers["push"]["tags"], ["v*"])
        self.assertEqual(triggers["pull_request"]["branches"], ["main"])
        self.assertIn("workflow_dispatch", triggers)
        validation = self.step(
            "python tools/build_android.py --target-dir target --codegen-units 8 "
            "--effects-acceptance --task testDebugUnitTest assembleDebug assembleDebugAndroidTest lintDebug"
        )
        self.assertNotIn("if", validation)
        acceptance = next(
            step for step in self.android["steps"]
            if step.get("with", {}).get("name") == "android-acceptance"
        )
        self.assertNotIn("if", acceptance)
        self.assertEqual(acceptance["with"]["if-no-files-found"], "error")
        self.assertEqual(self.workflow["jobs"]["ui"]["needs"], "android")

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

    def test_workflow_signing_packaging_upload_and_publish_follow_policy(self):
        policy = self.step("python tools/ci_release.py policy")
        self.assertEqual(policy["id"], "release_policy")
        self.assertNotIn("if", policy)
        self.assertLess(
            self.android["steps"].index(policy),
            self.android["steps"].index(self.step("python tools/ci_release.py prepare")),
        )
        prepare = self.step("python tools/ci_release.py prepare")
        package = self.step("python tools/ci_release.py package")
        upload = next(
            step for step in self.android["steps"]
            if step.get("with", {}).get("name") == "release-assets"
        )
        self.assertEqual(upload["with"]["if-no-files-found"], "error")
        preview = self.step(
            "python tools/build_android.py --target-dir target --codegen-units 8 --task assemblePreview"
        )
        release = self.step(
            "python tools/build_android.py --target-dir target --codegen-units 8 --task assembleRelease"
        )
        publish = self.workflow["jobs"]["publish"]
        self.assertEqual(publish["needs"], ["checks", "desktop", "desktop-media", "android", "ui"])
        for event, ref, kind in CASES:
            with self.subTest(event=event, ref=ref):
                context = {
                    "github": {"event_name": event, "ref": ref},
                    "steps": {"release_policy": {"outputs": {
                        "kind": kind, "publish": str(kind != "validation").lower(),
                    }}},
                }
                job_output = expression_value(self.android["outputs"]["publish"], context)
                context["needs"] = {"android": {"outputs": {"publish": job_output}}}
                for step in [prepare, package, upload, publish]:
                    self.assertEqual(expression_value(step.get("if"), context), kind != "validation")
                self.assertEqual(expression_value(preview.get("if"), context), kind == "preview")
                self.assertEqual(expression_value(release.get("if"), context), kind == "release")

    def test_missing_policy_outputs_do_not_enable_signing_or_publish(self):
        context = {
            "steps": {"release_policy": {"outputs": {"kind": "", "publish": ""}}},
            "needs": {"android": {"outputs": {"publish": ""}}},
        }
        for step in self.android["steps"]:
            if "release_policy.outputs" in step.get("if", ""):
                self.assertFalse(expression_value(step["if"], context))
        self.assertFalse(expression_value(self.workflow["jobs"]["publish"]["if"], context))


if __name__ == "__main__":
    unittest.main()
