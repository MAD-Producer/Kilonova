"""Prepare pinned shared LGPL FFmpeg libraries for desktop release builds.

Windows uses the checksum-verified BtbN LGPL shared distribution. Unix builds
the pinned upstream source with external dependency autodetection disabled.
The resulting manifest is consumed by the packager; no host library is guessed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import shlex
import subprocess
import tarfile
import urllib.request
import zipfile

WINDOWS_URL = "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-09-30-13-08/ffmpeg-n8.1.3-9-g29e619e767-win64-lgpl-shared-8.1.zip"
WINDOWS_SHA256 = "3e47bda1607740550141e37c0e49d1e5182b34699f15adfd137ee266d346811a"
SOURCE_URL = "https://codeload.github.com/FFmpeg/FFmpeg/tar.gz/refs/tags/n8.1.3"
SOURCE_SHA256 = "09f990289327d3ebfedf4fc15cae3f884a65dab7a5aac7bbb5b01ff83900efd9"
ZLIB_URL = "https://zlib.net/fossils/zlib-1.3.2.tar.gz"
ZLIB_SHA256 = "bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16"
PLATFORMS = ("windows-x86_64", "linux-x86_64", "macos-arm64", "macos-x86_64")


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def download(url, expected, destination):
    destination = Path(destination)
    if not destination.is_file() or digest(destination) != expected:
        partial = destination.with_suffix(destination.suffix + ".partial")
        with urllib.request.urlopen(url, timeout=60) as source, partial.open("wb") as out:
            shutil.copyfileobj(source, out)
        if digest(partial) != expected:
            raise ValueError(f"FFmpeg download checksum mismatch: {url}")
        partial.replace(destination)
    return destination


def prepare_zlib(platform, output, cache, license_dir):
    # Explicitly link a pinned static PIC library. Autodetection stays disabled;
    # the resulting libav must not depend on Homebrew or a build-prefix libz.
    archive_path = download(ZLIB_URL, ZLIB_SHA256, cache / "zlib-1.3.2.tar.gz")
    source_root = output / "zlib-source"
    source_root.mkdir(exist_ok=True)
    with tarfile.open(archive_path) as archive:
        archive.extractall(source_root, filter="data")
    source = source_root / "zlib-1.3.2"
    prefix = output / "zlib-prefix"
    env = dict(os.environ)
    env["CFLAGS"] = (env.get("CFLAGS", "") + " -O2 -fPIC").strip()
    if platform.startswith("macos-"):
        arch = "arm64" if platform == "macos-arm64" else "x86_64"
        env["CC"] = "clang -arch " + arch
    subprocess.run(["sh", str(source / "configure"), "--static", "--prefix=" + str(prefix)],
                   cwd=source, env=env, check=True)
    subprocess.run(["make", "-j" + str(min(os.cpu_count() or 2, 4))], cwd=source, env=env, check=True)
    subprocess.run(["make", "check"], cwd=source, env=env, check=True)
    subprocess.run(["make", "install"], cwd=source, env=env, check=True)
    if not (prefix / "lib/libz.a").is_file():
        raise ValueError("Pinned static zlib library is missing")
    shutil.copy2(source / "LICENSE", license_dir / "zlib-LICENSE.txt")
    provenance = {"name": "zlib", "author": "Jean-loup Gailly and Mark Adler",
                  "url": ZLIB_URL, "sha256": ZLIB_SHA256, "revision": "1.3.2",
                  "license": "Zlib", "linkage": "static PIC",
                  "modifications": "No source changes; static library linked into FFmpeg"}
    return prefix, provenance


def prepare(platform, output, cache):
    output, cache = Path(output).resolve(), Path(cache).resolve()
    output.mkdir(parents=True, exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    source_archive = download(SOURCE_URL, SOURCE_SHA256, cache / "ffmpeg-n8.1.3.tar.gz")
    source_root = output / "source"
    source_root.mkdir(exist_ok=True)
    with tarfile.open(source_archive) as archive:
        archive.extractall(source_root, filter="data")
    source = source_root / "FFmpeg-n8.1.3"
    prefix = output / "prefix"
    license_dir = prefix / "licenses"
    license_dir.mkdir(parents=True, exist_ok=True)
    config = []
    dependencies = []
    if platform == "windows-x86_64":
        binary_archive = download(WINDOWS_URL, WINDOWS_SHA256, cache / "ffmpeg-windows.zip")
        extracted = output / "windows"
        with zipfile.ZipFile(binary_archive) as archive:
            # Pinned trusted archive, still reject traversal and absolute members.
            for name in archive.namelist():
                path = Path(name)
                if path.is_absolute() or ".." in path.parts or "\\" in name:
                    raise ValueError("Invalid FFmpeg archive member")
            archive.extractall(extracted)
        distribution = extracted / "ffmpeg-n8.1.3-9-g29e619e767-win64-lgpl-shared-8.1"
        for directory in ("bin", "lib", "include"):
            shutil.copytree(distribution / directory, prefix / directory, dirs_exist_ok=True)
        shutil.copy2(distribution / "LICENSE.txt", license_dir / "FFmpeg-LGPL-3.0.txt")
        configuration = subprocess.run([str(prefix / "bin/ffmpeg.exe"), "-buildconf"],
                                       capture_output=True, text=True, check=True).stderr
        config = shlex.split(configuration.split("configuration:", 1)[1].splitlines()[0])
        if "--enable-gpl" in config or "--enable-nonfree" in config:
            raise ValueError("The Windows distribution is not the expected LGPL build")
        libraries = sorted((prefix / "bin").glob("*.dll"))
        provenance = {"url": WINDOWS_URL, "sha256": WINDOWS_SHA256,
                      "revision": "n8.1.3-9-g29e619e767", "license": "LGPL-3.0-or-later",
                      "buildScripts": "https://github.com/BtbN/FFmpeg-Builds/tree/6c9aec5fc9a72ec3abedd1fa84db141fa18cf52b",
                      "source": "https://github.com/FFmpeg/FFmpeg/tree/29e619e767"}
    else:
        zlib_prefix, zlib_provenance = prepare_zlib(platform, output, cache, license_dir)
        dependencies.append(zlib_provenance)
        config = ["--prefix=" + str(prefix), "--enable-shared", "--disable-static",
                  "--disable-autodetect", "--disable-gpl", "--disable-nonfree",
                  "--enable-zlib", "--extra-cflags=-I" + shlex.quote(str(zlib_prefix / "include")),
                  "--extra-ldflags=-L" + shlex.quote(str(zlib_prefix / "lib")),
                  "--disable-programs", "--disable-doc", "--disable-debug",
                  "--disable-avdevice", "--disable-avfilter", "--disable-network",
                  "--disable-encoders", "--disable-muxers", "--disable-x86asm"]
        if platform.startswith("macos-"):
            arch = "arm64" if platform == "macos-arm64" else "x86_64"
            config += ["--target-os=darwin", "--arch=" + arch, "--enable-cross-compile",
                       "--cc=clang -arch " + arch, "--ld=clang -arch " + arch,
                       "--install-name-dir=@rpath"]
        env = dict(os.environ)
        env["PKG_CONFIG_PATH"] = str(zlib_prefix / "lib/pkgconfig") + os.pathsep + env.get("PKG_CONFIG_PATH", "")
        subprocess.run(["sh", str(source / "configure"), *config], cwd=source, env=env, check=True)
        if "#define CONFIG_ZLIB 1" not in (source / "config.h").read_text(encoding="utf-8"):
            raise ValueError("FFmpeg configuration did not enable required compressed MOV support (zlib)")
        subprocess.run(["make", "-j" + str(min(os.cpu_count() or 2, 4))], cwd=source, check=True)
        subprocess.run(["make", "install"], cwd=source, check=True)
        pattern = "*.dylib" if platform.startswith("macos-") else "*.so*"
        # Preserve SONAME aliases as regular files so archive extraction needs
        # neither symlink privileges nor the original installation prefix.
        libraries = sorted(p for p in (prefix / "lib").glob(pattern) if p.is_file())
        provenance = {"url": SOURCE_URL, "sha256": SOURCE_SHA256, "revision": "n8.1.3",
                      "source": "https://github.com/FFmpeg/FFmpeg/tree/n8.1.3",
                      "license": "LGPL-2.1-or-later"}
    if len(libraries) < 4:
        raise ValueError("FFmpeg runtime libraries are incomplete")
    for name in ("COPYING.LGPLv2.1", "COPYING.LGPLv3", "COPYING.GPLv3", "LICENSE.md"):
        shutil.copy2(source / name, license_dir / name)
    files = [{"path": p.relative_to(prefix).as_posix(), "sha256": digest(p)} for p in libraries]
    licenses = [{"path": p.relative_to(prefix).as_posix(), "sha256": digest(p)} for p in sorted(license_dir.iterdir())]
    manifest = {"schemaVersion": 1, "platform": platform, "author": "FFmpeg developers",
                "provenance": provenance, "configure": config,
                "modifications": "No source changes; shared libraries copied without the command-line programs",
                "files": files, "licenses": licenses, "dependencies": dependencies}
    (prefix / "runtime.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return prefix


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", required=True, choices=PLATFORMS)
    parser.add_argument("--output", default="artifacts/ffmpeg")
    parser.add_argument("--cache", default="artifacts/ffmpeg-downloads")
    args = parser.parse_args()
    prefix = prepare(args.platform, args.output, args.cache)
    if env_file := os.environ.get("GITHUB_ENV"):
        with open(env_file, "a", encoding="utf-8") as env:
            env.write(f"FFMPEG_DIR={prefix}\n")
            env.write(f"PKG_CONFIG_PATH={prefix / 'lib/pkgconfig'}\n")
            env.write(f"LD_LIBRARY_PATH={prefix / 'lib'}\n")
            env.write(f"DYLD_LIBRARY_PATH={prefix / 'lib'}\n")
    if path_file := os.environ.get("GITHUB_PATH"):
        with open(path_file, "a", encoding="utf-8") as path:
            path.write(str(prefix / "bin") + "\n")
    print(prefix)


if __name__ == "__main__":
    main()
