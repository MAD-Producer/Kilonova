"""CI regressions for build entrypoint arguments without invoking compilers."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location("motion_android_build", Path(__file__).with_name("build_android.py"))
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)


class AndroidBuildTests(unittest.TestCase):
    def test_selected_abi_and_isolated_debug_reach_gradle_after_native_build(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = {"java_home": str(root/"jdk"), "sdk": str(root/"sdk"),
                      "gradle": "test-gradle", "ndk": str(root/"ndk")}
            calls = []

            def run(command, **kwargs):
                calls.append(command)
                if command[:2] == ["cargo", "build"]:
                    target = command[command.index("--target") + 1]
                    library = Path(kwargs["env"]["CARGO_TARGET_DIR"])/target/"release/libmotion_android.so"
                    library.parent.mkdir(parents=True, exist_ok=True)
                    library.write_bytes(b"native build fixture")
                return subprocess.CompletedProcess(command, 0)

            with mock.patch.object(build, "ROOT", root), mock.patch.object(build, "tool_config", return_value=(root, config)), \
                    mock.patch.object(build.sys, "platform", "linux"), \
                    mock.patch.object(build.sys, "argv", ["build_android.py", "--abis", "x86_64", "--isolated-debug"]), \
                    mock.patch.object(build.subprocess, "check_output", return_value="x86_64-linux-android\n"), \
                    mock.patch.object(build.subprocess, "run", side_effect=run):
                build.main()
            self.assertEqual(len(calls), 2)
            self.assertEqual(calls[0][calls[0].index("--target")+1], "x86_64-linux-android")
            self.assertEqual(calls[1], ["test-gradle", "--no-daemon", "--console=plain",
                "-PmotionAbis=x86_64", "-PisolatedDebug=true", "assembleDebug"])
            self.assertTrue((root/"android/app/src/main/jniLibs/x86_64/libmotion_engine.so").is_file())
            self.assertFalse((root/"android/app/src/main/jniLibs/arm64-v8a").exists())

    def test_invalid_variant_flags_refuse_before_any_build(self):
        cases = [
            ["--isolated-debug", "--task", "assembleRelease"],
            ["--isolated-debug", "--task", "assemblePreview"],
            ["--isolated-debug", "--effects-acceptance"],
            ["--diagnostics", "--task", "assembleBenchmark"],
            ["--abis", "armeabi-v7a"],
        ]
        for arguments in cases:
            with self.subTest(arguments=arguments), \
                    mock.patch.object(build.sys, "argv", ["build_android.py", *arguments]), \
                    mock.patch.object(build, "tool_config") as tools, \
                    mock.patch.object(build.subprocess, "run") as run:
                with self.assertRaises(RuntimeError):
                    build.main()
                tools.assert_not_called()
                run.assert_not_called()

    def test_explicit_ndk_override_applies_to_shared_configuration_without_rewriting_it(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/".tools").mkdir()
            path = root/".tools/environment.json"
            path.write_text(json.dumps({"ndk": "pinned-27", "java_home": "jdk"}), encoding="utf-8")
            original = path.read_bytes()
            with mock.patch.object(build, "ROOT", root), mock.patch.dict(build.os.environ, {"ANDROID_NDK_HOME": "override-ndk"}):
                shared, config = build.tool_config()
            self.assertEqual(shared, root)
            self.assertEqual(config["ndk"], "override-ndk")
            self.assertEqual(path.read_bytes(), original)
            with mock.patch.object(build, "ROOT", root), mock.patch.dict(build.os.environ, {"ANDROID_NDK_HOME": ""}):
                self.assertEqual(build.tool_config()[1]["ndk"], "pinned-27")


if __name__ == "__main__":
    unittest.main()
