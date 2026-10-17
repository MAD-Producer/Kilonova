"""Package the verified desktop builds and validate their release manifests."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PLATFORMS = {
    "windows-x86_64": ("motion-studio.exe", ".zip"),
    "linux-x86_64": ("motion-studio", ".tar.gz"),
    "macos-arm64": ("MotionStudio.app/Contents/MacOS/motion-studio", ".zip"),
    "macos-x86_64": ("MotionStudio.app/Contents/MacOS/motion-studio", ".zip"),
}


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def archive_name(platform, kind, version):
    token = kind if kind in ("preview", "validation") else version
    if not token or any(c not in "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-" for c in token):
        raise ValueError("Invalid desktop release version")
    return f"MotionStudio-{token}-{platform}{PLATFORMS[platform][1]}"


def verify_binary(binary, platform):
    with Path(binary).open("rb") as source:
        header = source.read(64)
        if platform == "windows-x86_64":
            if len(header) != 64 or header[:2] != b"MZ":
                raise ValueError("Desktop binary is not a Windows executable")
            source.seek(struct.unpack_from("<I", header, 60)[0])
            pe = source.read(6)
            if pe != b"PE\0\0\x64\x86":
                raise ValueError("Desktop binary is not Windows x86_64")
        elif platform == "linux-x86_64":
            if len(header) != 64 or header[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", header, 18)[0] != 62:
                raise ValueError("Desktop binary is not Linux x86_64 ELF")
        elif platform.startswith("macos-"):
            cpu = 0x0100000C if platform == "macos-arm64" else 0x01000007
            if len(header) != 64 or struct.unpack_from("<II", header) != (0xFEEDFACF, cpu):
                raise ValueError("Desktop Mach-O architecture differs from its package")
        else:
            raise ValueError("Unsupported desktop platform")


def notes(platform):
    opening = {
        "windows-x86_64": "解压后双击 motion-studio.exe（Windows 10/11，x64）",
        "linux-x86_64": "解压后运行 ./motion-studio（Ubuntu 24.04 x86_64；需要系统图形驱动、ALSA 与 XKB 运行库）",
        "macos-arm64": "解压后将 MotionStudio.app 拖入应用程序（Apple Silicon，macOS 11+）；本预览使用 ad-hoc 签名，未做 Developer ID 公证",
        "macos-x86_64": "解压后将 MotionStudio.app 拖入应用程序（Intel，macOS 11+）；本预览使用 ad-hoc 签名，未做 Developer ID 公证",
    }[platform]
    return (
        f"Motion Studio 桌面预览版\n\n{opening}\n\n"
        "此包包含 FFmpeg 视频导入/解码后端及共享运行库；"
        "工程编辑、图片与可用音频格式、效果、节点及 MCP 使用共享后端。"
        "构建能力以 build-info.json 为准。\n"
    )


def runtime_payload(manifest_path, platform):
    """Verify every bundled runtime file, including architecture and provenance."""
    manifest_path = Path(manifest_path)
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("schemaVersion") != 1 or manifest.get("platform") != platform:
        raise ValueError("FFmpeg runtime platform mismatch")
    provenance = manifest.get("provenance", {})
    if not all(provenance.get(k) for k in ("url", "sha256", "source", "revision", "license")):
        raise ValueError("FFmpeg runtime provenance is incomplete")
    if not manifest.get("author") or not manifest.get("modifications"):
        raise ValueError("FFmpeg runtime attribution is incomplete")
    files, names = [], set()
    for key in ("files", "licenses"):
        entries = manifest.get(key, [])
        if not entries:
            raise ValueError("FFmpeg runtime files or licenses are missing")
        for entry in entries:
            relative = Path(entry["path"])
            if relative.is_absolute() or ".." in relative.parts or "\\" in entry["path"]:
                raise ValueError("Invalid FFmpeg runtime path")
            source = manifest_path.parent / relative
            if not source.is_file() or digest(source) != entry.get("sha256"):
                raise ValueError("FFmpeg runtime file checksum mismatch")
            if source.name in names:
                raise ValueError("Duplicate FFmpeg runtime filename")
            names.add(source.name)
            if key == "files":
                verify_binary(source, platform)
            files.append((source, key))
    required = ("avcodec", "avformat", "avutil", "swscale")
    library_names = [Path(e["path"]).name for e in manifest["files"]]
    if not all(any(n.startswith(component + "-") or n.startswith("lib" + component + ".") for n in library_names) for component in required):
        raise ValueError("FFmpeg decoder library set is incomplete")
    return manifest, files


def package(binary, platform, build_report, output, *, sign=True):
    from ci_release import build_kind, release_kind, version_code
    binary = Path(binary).resolve()
    if not binary.is_file():
        raise ValueError("Desktop executable is missing")
    verify_binary(binary, platform)
    build = json.loads(Path(build_report).read_text(encoding="utf-8"))
    if build.get("task") != "build" or build.get("release") is not True or build.get("ffmpeg") is not True:
        raise ValueError("Release packaging requires an FFmpeg-enabled release build")
    if Path(build.get("executable", "")).resolve() != binary:
        raise ValueError("Build report does not describe this executable")
    if not build.get("ffmpeg_runtime"):
        raise ValueError("FFmpeg-enabled release has no runtime manifest")
    runtime, runtime_files = runtime_payload(build["ffmpeg_runtime"], platform)
    kind = build_kind(os.environ["GITHUB_EVENT_NAME"], os.environ["GITHUB_REF"])
    base_version = (release_kind(os.environ["GITHUB_REF"])[1] if kind != "validation"
                    else tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"])
    version = base_version
    if kind in ("preview", "validation"):
        version += "-" + kind + "." + str(version_code(os.environ["GITHUB_RUN_NUMBER"]))
    info = {
        "schemaVersion": 1, "platform": platform, "kind": kind, "versionName": version,
        "commit": os.environ["GITHUB_SHA"], "ref": os.environ["GITHUB_REF"],
        "run": os.environ["GITHUB_RUN_ID"], "capabilities": {"ffmpeg": True, "videoEncoder": False},
        "archive": archive_name(platform, kind, version),
    }
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="motion-desktop-", dir=output) as temporary:
        staging = Path(temporary) / "payload"
        target = staging / PLATFORMS[platform][0]
        target.parent.mkdir(parents=True)
        shutil.copy2(binary, target)
        target.chmod(0o755)
        (staging / "README.txt").write_text(notes(platform), encoding="utf-8")
        shutil.copy2(ROOT / "LICENSE", staging / "LICENSE")
        shutil.copy2(ROOT / "crates/motion-effects/motion-library/LICENSE.shaders.txt", staging / "LICENSE.shaders.txt")
        shutil.copy2(ROOT / "ATTRIBUTION.md", staging / "ATTRIBUTION.md")
        shutil.copy2(ROOT / "NOTICE", staging / "NOTICE")
        native_dir = (target.parent if platform == "windows-x86_64" else
                      staging / "MotionStudio.app/Contents/Frameworks" if platform.startswith("macos-") else staging / "lib")
        native_dir.mkdir(parents=True, exist_ok=True)
        bundled = []
        for source, kind in runtime_files:
            destination = native_dir / source.name if kind == "files" else staging / "licenses/ffmpeg" / source.name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
            bundled.append(destination)
        (staging / "FFmpeg-SOURCE.json").write_text(json.dumps(runtime, indent=2) + "\n", encoding="utf-8")
        if platform.startswith("macos-"):
            app = staging / "MotionStudio.app"
            plist = {
                "CFBundleName": "Motion Studio", "CFBundleDisplayName": "Motion Studio",
                "CFBundleIdentifier": "com.motionstudio.desktop", "CFBundleExecutable": "motion-studio",
                "CFBundlePackageType": "APPL", "CFBundleShortVersionString": base_version,
                "CFBundleVersion": str(version_code(os.environ["GITHUB_RUN_NUMBER"])),
                "LSMinimumSystemVersion": "11.0", "NSHighResolutionCapable": True,
            }
            (app / "Contents/Info.plist").write_bytes(plistlib.dumps(plist))
            if sign:
                if sys.platform != "darwin":
                    raise ValueError("macOS app signing requires the macOS runner")
                for library in sorted(native_dir.glob("*.dylib")):
                    subprocess.run(["codesign", "--force", "--sign", "-", "--timestamp=none", str(library)], check=True)
                subprocess.run(["codesign", "--force", "--sign", "-", "--timestamp=none", str(app)], check=True)
                subprocess.run(["codesign", "--verify", "--strict", str(app)], check=True)
            info["signing"] = "ad-hoc" if sign else "test-only-unsigned"
            info["notarised"] = False
        info["runtimeFiles"] = [{"path": p.relative_to(staging).as_posix(), "sha256": digest(p)} for p in bundled]
        info["ffmpeg"] = runtime["provenance"]
        archive = Path(temporary) / info["archive"]
        if platform == "linux-x86_64":
            with tarfile.open(archive, "w:gz") as bundle:
                for source in sorted(staging.rglob("*")):
                    entry = bundle.gettarinfo(str(source), arcname=source.relative_to(staging).as_posix())
                    entry.mode = 0o755 if source == target or source.is_dir() else 0o644
                    entry.uid = entry.gid = 0
                    entry.uname = entry.gname = ""
                    if source.is_file():
                        with source.open("rb") as contents:
                            bundle.addfile(entry, contents)
                    else:
                        bundle.addfile(entry)
        else:
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
                for source in sorted(staging.rglob("*")):
                    if source.is_file():
                        entry = zipfile.ZipInfo(source.relative_to(staging).as_posix())
                        entry.create_system = 3
                        entry.external_attr = (stat.S_IFREG | (0o755 if source == target else 0o644)) << 16
                        entry.compress_type = zipfile.ZIP_DEFLATED
                        bundle.writestr(entry, source.read_bytes())
        info["sha256"] = digest(archive)
        info["bytes"] = archive.stat().st_size
        os.replace(archive, output / info["archive"])
    (output / f"build-info-{platform}.json").write_text(json.dumps(info, indent=2) + "\n", encoding="utf-8")
    return info


def collect_assets(folder, android):
    """Reject incomplete, stale or corrupted bundles before any release write."""
    folder = Path(folder)
    apk_name = android["apk"]
    if Path(apk_name).name != apk_name or "\\" in apk_name:
        raise ValueError("Invalid APK asset name")
    apk = folder / apk_name
    if digest(apk) != android["sha256"]:
        raise ValueError("Release APK checksum mismatch")
    assets = [{"platform": "android", "name": apk_name, "sha256": android["sha256"], "bytes": apk.stat().st_size}]
    return assets + collect_desktop_assets(folder, android)


def collect_desktop_assets(folder, release):
    """Validate the desktop matrix without requiring an APK or Android metadata."""
    folder = Path(folder)
    assets = []
    for platform in PLATFORMS:
        info = json.loads((folder / f"build-info-{platform}.json").read_text(encoding="utf-8"))
        if any(info.get(key) != release[key] for key in ("commit", "ref", "kind", "run", "versionName")):
            raise ValueError("Desktop artifact does not belong to this workflow revision")
        name = archive_name(platform, release["kind"], release["versionName"])
        if info.get("schemaVersion") != 1 or info.get("platform") != platform or info.get("archive") != name:
            raise ValueError("Desktop platform/archive mismatch")
        if info.get("capabilities") != {"ffmpeg": True, "videoEncoder": False}:
            raise ValueError("Unverified desktop runtime dependencies")
        if platform.startswith("macos-") and info.get("signing") != "ad-hoc":
            raise ValueError("macOS app bundle was not signed")
        path = folder / name
        if digest(path) != info.get("sha256") or path.stat().st_size != info.get("bytes"):
            raise ValueError("Desktop artifact checksum or size mismatch")
        runtime = info.get("runtimeFiles", [])
        if len(runtime) < 5 or not info.get("ffmpeg", {}).get("source"):
            raise ValueError("Desktop FFmpeg runtime evidence is missing")
        with (tarfile.open(path) if platform == "linux-x86_64" else zipfile.ZipFile(path)) as bundle:
            for entry in runtime:
                member = entry["path"]
                if Path(member).is_absolute() or ".." in Path(member).parts or "\\" in member:
                    raise ValueError("Invalid bundled FFmpeg runtime path")
                if platform == "linux-x86_64":
                    with bundle.extractfile(member) as contents:
                        actual = hashlib.file_digest(contents, "sha256").hexdigest()
                else:
                    with bundle.open(member) as contents:
                        actual = hashlib.file_digest(contents, "sha256").hexdigest()
                if actual != entry["sha256"]:
                    raise ValueError("Bundled FFmpeg runtime checksum mismatch")
        assets.append({"platform": platform, "name": name, "sha256": info["sha256"], "bytes": info["bytes"], "capabilities": info["capabilities"]})
    return assets


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--platform", choices=PLATFORMS, required=True)
    parser.add_argument("--build-report", required=True)
    parser.add_argument("--output", default="artifacts/desktop-release")
    args = parser.parse_args()
    print(json.dumps(package(args.binary, args.platform, args.build_report, args.output), indent=2))


if __name__ == "__main__":
    main()
