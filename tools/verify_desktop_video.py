"""Decode synthetic videos using only the runtime in an extracted release."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import sys
import zipfile

from package_desktop import PLATFORMS


def verify(archive, platform, output):
    archive, output = Path(archive).resolve(), Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env["MOTION_LOG"] = "info"
    for key in ("FFMPEG_DIR", "FFMPEG_LIB_DIR", "FFMPEG_INCLUDE_DIR", "LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH"):
        env.pop(key, None)
    if os.name == "nt":
        env["LOCALAPPDATA"] = str(output / "data")
        log_path = output / "data/MotionStudio/logs/motion-studio.log"
        system = Path(os.environ["SystemRoot"])
        env["PATH"] = os.pathsep.join(map(str, (system / "System32", system)))
    else:
        env["XDG_DATA_HOME"] = str(output / "data")
        log_path = (Path(env["HOME"]) / "Library/Application Support/MotionStudio/logs/motion-studio.log"
                    if sys.platform == "darwin" else output / "data/motion-studio/logs/motion-studio.log")
        env["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
    results = []
    fixtures = Path(__file__).resolve().parents[1] / "crates/motion-media/tests/fixtures/video"
    with tempfile.TemporaryDirectory(prefix="motion-packaged-video-") as folder:
        root = Path(folder)
        if platform == "linux-x86_64":
            with tarfile.open(archive) as bundle:
                bundle.extractall(root, filter="data")
        else:
            with zipfile.ZipFile(archive) as bundle:
                for name in bundle.namelist():
                    if Path(name).is_absolute() or ".." in Path(name).parts or "\\" in name:
                        raise ValueError("Invalid desktop archive member")
                bundle.extractall(root)
        binary = root / PLATFORMS[platform][0]
        binary.chmod(0o755)
        for name in ("silent-24fps.mp4", "variable.mp4", "rotated-90.mp4", "video-tail-cmov.mov"):
            result = subprocess.run([str(binary), "--video-smoke", str(fixtures / name)], cwd=root,
                                    env=env, capture_output=True, text=True, encoding="utf-8", timeout=60)
            (output / (name + ".stderr.log")).write_text(result.stderr, encoding="utf-8")
            if result.returncode:
                raise RuntimeError(f"Packaged {platform} video decode failed ({name}): {result.stderr}")
            report = json.loads(result.stdout)
            if report.get("decoded") is not True or report.get("backend") != "libav" or len(report.get("frames", [])) != 4:
                raise ValueError("Packaged executable did not prove video decoding")
            results.append({"fixture": name, **report})
        # Exercise the real failure route at the default log level. The CLI must
        # preserve the source/phase/cause on disk and stderr without corrupting
        # its JSON stdout channel.
        missing = root / "controlled-missing-video.mp4"
        failed = subprocess.run([str(binary), "--video-smoke", str(missing)], cwd=root,
                                env=env, capture_output=True, text=True, encoding="utf-8", timeout=60)
        file_log = log_path.read_text(encoding="utf-8")
        (output / "failure.stderr.log").write_text(failed.stderr, encoding="utf-8")
        (output / "failure.file.log").write_text(file_log, encoding="utf-8")
        if failed.returncode == 0 or failed.stdout:
            raise ValueError("Video runtime failure was swallowed or polluted stdout")
        for content in (failed.stderr, file_log):
            if not all(marker in content for marker in ("ERROR", "video.runtime", "phase=probe", missing.name, "cause=")):
                raise ValueError("Video runtime failure did not retain its diagnostic context")
    (output / "video-runtime.json").write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--platform", choices=PLATFORMS, required=True)
    parser.add_argument("--output", default="artifacts/desktop/video-runtime")
    args = parser.parse_args()
    print(json.dumps(verify(args.archive, args.platform, args.output), indent=2))


if __name__ == "__main__":
    main()
