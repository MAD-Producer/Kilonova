"""Build the pinned native OCIO runtime; never discover an installed OCIO.

The output directory is exclusive. Failed builds retain their original logs and
in-flight state. A runtime manifest is published only after successful install,
architecture verification, and inclusion of every dependency's original license.
This recipe does not enable managed rendering or alter numerical references.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform as host_platform
import shutil
import subprocess
import sys
import tarfile
import time
import urllib.request

LOCK = Path(__file__).with_name("ocio_sources.json")
PLATFORMS = ("windows-x86_64", "linux-x86_64", "macos-x86_64", "macos-arm64")


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def sources(platform=None):
    document = json.loads(LOCK.read_text(encoding="utf-8"))
    if document["schemaVersion"] != 1:
        raise ValueError("Unsupported OCIO source lock")
    return [item for item in document["components"]
            if platform is None or platform in item.get("platforms", PLATFORMS)]


def runtime_identity(manifest, platform):
    """Check pinned identities independently of a local runtime directory."""
    if (manifest.get("schemaVersion") != 1 or manifest.get("version") != "2.6.0"
            or platform not in PLATFORMS or manifest.get("platform") != platform
            or manifest.get("components") != sources(platform)
            or manifest.get("sourceLockSha256") != digest(LOCK)
            or manifest.get("bridgeSourceSha256") != digest(Path(__file__).with_name("ocio-native") / "bridge.cpp")):
        raise ValueError("Native OCIO runtime source, bridge or platform differs")
    entries, licenses = manifest.get("files", []), manifest.get("licenses", [])
    if not 2 <= len(entries) <= 8 or len(licenses) != len(sources(platform)):
        raise ValueError("Native OCIO library or license set incomplete")
    expected = {f"licenses/ocio/{item['name']}-LICENSE.txt": item['license_sha256']
                for item in sources(platform)}
    if {item.get("path"): item.get("sha256") for item in licenses} != expected:
        raise ValueError("Native OCIO dependency licenses differ from pinned source")
    names = set()
    core, bridge = 0, 0
    for item in entries + licenses:
        name = item.get("path", "")
        path = PurePosixPath(name)
        if (not name or path.is_absolute() or path.as_posix() != name
                or ".." in path.parts or ":" in name or "\\" in name or name in names):
            raise ValueError("Native OCIO path is not unique, canonical and relative")
        names.add(name)
        if item in entries:
            if platform == "windows-x86_64":
                is_core = path.parent == PurePosixPath("bin") and path.name == "OpenColorIO_2_6.dll"
                is_bridge = name == "bin/motion_ocio_bridge.dll"
            else:
                is_core = (path.parent == PurePosixPath("lib") and path.name.startswith("libOpenColorIO.")
                           and (path.name.endswith(".dylib") if platform.startswith("macos-") else ".so." in path.name))
                is_bridge = name == ("lib/libmotion_ocio_bridge.dylib" if platform.startswith("macos-")
                                     else "lib/libmotion_ocio_bridge.so")
            if not is_core and not is_bridge:
                raise ValueError("Unexpected native OCIO runtime library")
            core += is_core
            bridge += is_bridge
    if not core or bridge != 1:
        raise ValueError("Native OCIO core or bridge library missing")


def runtime_payload(manifest_path, platform):
    """Qualify actual files before bundling; never load a library during packaging."""
    from package_desktop import verify_binary
    manifest_path = Path(manifest_path).resolve(strict=True)
    if manifest_path.stat().st_size > 65536:
        raise ValueError("Native OCIO manifest exceeds capacity")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    runtime_identity(manifest, platform)
    root = manifest_path.parent
    files = []
    for category in ("files", "licenses"):
        for entry in manifest[category]:
            path = (root / entry["path"]).resolve(strict=True)
            if not path.is_relative_to(root) or path.stat().st_size > 64 * 1024 * 1024:
                raise ValueError("Native OCIO file escapes its prefix or exceeds capacity")
            if digest(path) != entry.get("sha256"):
                raise ValueError("Native OCIO file checksum mismatch")
            if category == "files":
                verify_binary(path, platform)
            files.append((path, entry["path"]))
    return manifest, files


def download(component, cache, offline):
    path = cache / (component["name"] + "-" + component["commit"] + ".tar.gz")
    if not path.exists():
        if offline:
            raise FileNotFoundError(f"OCIO offline source missing: {path}")
        partial = path.with_suffix(".partial")
        # Never replace a retained failed download or silently retry a bad cache.
        with urllib.request.urlopen(component["url"], timeout=60) as source, partial.open("xb") as out:
            shutil.copyfileobj(source, out)
        verify_archive(partial, component)
        partial.rename(path)
    verify_archive(path, component)
    return path


def verify_archive(path, component):
    if path.stat().st_size != component["bytes"] or digest(path) != component["sha256"]:
        raise ValueError(f"OCIO source checksum mismatch: {component['name']}")


def extract(archive, destination):
    destination.mkdir()
    with tarfile.open(archive) as source:
        members = source.getmembers()
        if len(members) > 25000 or sum(m.size for m in members) > 1024 * 1024 * 1024:
            raise ValueError("OCIO source archive exceeds extraction capacity")
        for member in members:
            name = PurePosixPath(member.name)
            if name.is_absolute() or ".." in name.parts or ":" in member.name or "\\" in member.name:
                raise ValueError("OCIO source archive path escapes its directory")
            # data_filter additionally checks symlink/hardlink destinations.
            tarfile.data_filter(member, str(destination))
        source.extractall(destination, members=members, filter="data")
    roots = list(destination.iterdir())
    if len(roots) != 1 or not roots[0].is_dir():
        raise ValueError("OCIO source archive must have one root directory")
    return roots[0]


class Build:
    def __init__(self, output, platform, cmake, ninja, jobs):
        self.output, self.platform = output, platform
        self.cmake, self.ninja, self.jobs = cmake, ninja, jobs
        self.prefix = output / "prefix"
        self.identity = {str(path): digest(path) for path in
                         (Path(__file__), LOCK, Path(__file__).with_name("ocio-native") / "bridge.cpp",
                          Path(__file__).with_name("ocio-native") / "CMakeLists.txt")}
        (output / "logs").mkdir()

    def event(self, **value):
        with (self.output / "events.jsonl").open("a", encoding="utf-8") as log:
            log.write(json.dumps(value, ensure_ascii=False) + "\n")

    def run(self, component, phase, command):
        context = dict(component=component, phase=phase, command=command)
        started = time.monotonic()
        state = self.output / "in-flight.json"
        state.write_text(json.dumps(dict(**context, running=True)), encoding="utf-8")
        self.event(level="info", operation="start", **context)
        stem = self.output / "logs" / (component + "-" + phase)
        completed = False
        try:
            with stem.with_suffix(".stdout.txt").open("xb") as out, stem.with_suffix(".stderr.txt").open("xb") as err:
                result = subprocess.run(command, stdout=out, stderr=err, check=False)
            completed = True
            elapsed = int((time.monotonic() - started) * 1000)
            self.event(level="error" if result.returncode else "debug", operation="done",
                       elapsed_ms=elapsed, exit_code=result.returncode, **context)
            if result.returncode:
                message = f"OCIO component={component} phase={phase} elapsed_ms={elapsed} exit={result.returncode}; logs={stem}"
                print(message, file=sys.stderr)
                raise subprocess.CalledProcessError(result.returncode, command)
        finally:
            if not completed:
                self.event(level="error", operation="abandoned", elapsed_ms=int((time.monotonic()-started)*1000), **context)
            state.write_text(json.dumps(dict(**context, running=False)), encoding="utf-8")

    def configure_install(self, name, source, flags):
        directory = self.output / "build" / name
        common = ["-G", "Ninja", f"-DCMAKE_MAKE_PROGRAM={self.ninja}", "-DCMAKE_BUILD_TYPE=Release",
                  f"-DCMAKE_INSTALL_PREFIX={self.prefix.as_posix()}", f"-DCMAKE_PREFIX_PATH={self.prefix.as_posix()}",
                  "-DCMAKE_INSTALL_LIBDIR=lib", "-DCMAKE_POSITION_INDEPENDENT_CODE=ON",
                  "-DCMAKE_FIND_USE_PACKAGE_REGISTRY=OFF", "-DCMAKE_FIND_USE_SYSTEM_PACKAGE_REGISTRY=OFF",
                  "-DBUILD_SHARED_LIBS=OFF", "-DBUILD_TESTING=OFF"]
        if self.platform == "windows-x86_64":
            common += ["-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL", "-DCMAKE_C_FLAGS=/utf-8", "-DCMAKE_CXX_FLAGS=/utf-8"]
            for variable, executable in (("CMAKE_RC_COMPILER", "rc.exe"), ("CMAKE_MT", "mt.exe")):
                common.append(f"-D{variable}={Path(required_tool(executable)).as_posix()}")
        if self.platform.startswith("macos-"):
            common += ["-DCMAKE_OSX_ARCHITECTURES=" + ("arm64" if self.platform.endswith("arm64") else "x86_64"),
                       "-DCMAKE_OSX_DEPLOYMENT_TARGET=11.0", "-DCMAKE_INSTALL_NAME_DIR=@rpath"]
        self.run(name, "configure", [self.cmake, "-S", str(source), "-B", str(directory), *common, *flags])
        self.run(name, "install", [self.cmake, "--build", str(directory), "--target", "install", "--parallel", str(self.jobs)])

    def compile(self, source):
        prefix = self.prefix.as_posix()
        self.configure_install("zlib", source["zlib"], ["-DZLIB_BUILD_SHARED=OFF", "-DZLIB_BUILD_TESTING=OFF"])
        zlib = self.prefix / "lib" / ("zs.lib" if self.platform == "windows-x86_64" else "libz.a")
        if not zlib.is_file():
            raise FileNotFoundError(f"Pinned static zlib missing: {zlib}")
        zflags = [f"-DZLIB_LIBRARY={zlib.as_posix()}", f"-DZLIB_INCLUDE_DIR={prefix}/include"]
        self.configure_install("expat", source["expat"] / "expat", ["-DEXPAT_BUILD_DOCS=OFF", "-DEXPAT_BUILD_EXAMPLES=OFF",
                               "-DEXPAT_BUILD_TESTS=OFF", "-DEXPAT_BUILD_TOOLS=OFF", "-DEXPAT_SHARED_LIBS=OFF"])
        self.configure_install("yaml-cpp", source["yaml-cpp"], ["-DYAML_BUILD_SHARED_LIBS=OFF", "-DYAML_CPP_BUILD_TESTS=OFF",
                               "-DYAML_CPP_BUILD_TOOLS=OFF", "-DYAML_CPP_BUILD_CONTRIB=OFF"])
        self.configure_install("Imath", source["Imath"], ["-DPYTHON=OFF", "-DDOCS=OFF", "-DIMATH_HALF_USE_LOOKUP_TABLE=OFF"])
        wrapper = self.output / "pystring-wrapper"
        wrapper.mkdir()
        original = source["pystring"].as_posix()
        (wrapper / "CMakeLists.txt").write_text(
            'cmake_minimum_required(VERSION 3.18)\nproject(MotionPystringDependency LANGUAGES CXX)\n'
            f'add_library(pystring STATIC "{original}/pystring.cpp")\n'
            'install(TARGETS pystring ARCHIVE DESTINATION lib)\n'
            f'install(FILES "{original}/pystring.h" DESTINATION include/pystring)\n', encoding="utf-8")
        self.configure_install("pystring", wrapper, [])
        disabled = ("OPENSSL", "LIBBSD", "BUILD_TESTS", "COMPAT", "BZIP2", "LZMA", "LIBCOMP", "ZSTD", "PPMD",
                    "PKCRYPT", "WZAES", "SIGNING", "ICONV", "FETCH_LIBS", "FORCE_FETCH_LIBS")
        self.configure_install("minizip-ng", source["minizip-ng"], [f"-DMZ_{name}=OFF" for name in disabled] + ["-DMZ_ZLIB=ON", *zflags])
        # All third-party dependencies must come from this verified prefix.
        roots = [f"-D{name}_ROOT={prefix}" for name in ("expat", "yaml-cpp", "pystring", "Imath", "minizip-ng")]
        arm = ([f"-Dsse2neon_INCLUDE_DIR={source['sse2neon'].as_posix()}"]
               if self.platform == "macos-arm64" else [])
        self.configure_install("OpenColorIO", source["OpenColorIO"], ["-DBUILD_SHARED_LIBS=ON", "-DOCIO_INSTALL_EXT_PACKAGES=NONE",
                               "-DOCIO_BUILD_APPS=OFF", "-DOCIO_BUILD_TESTS=OFF", "-DOCIO_BUILD_GPU_TESTS=OFF",
                               "-DOCIO_BUILD_DOCS=OFF", "-DOCIO_BUILD_PYTHON=OFF", "-DOCIO_BUILD_JAVA=OFF",
                               "-DOCIO_BUILD_OPENFX=OFF", "-DOCIO_BUILD_NUKE=OFF", "-DOCIO_DIRECTX_ENABLED=OFF",
                               "-DOCIO_VULKAN_ENABLED=OFF", "-DOCIO_USE_AVX512=OFF", "-DZLIB_USE_STATIC_LIBS=ON", *zflags, *roots, *arm])
        self.configure_install("motion-ocio-bridge", Path(__file__).with_name("ocio-native"), [f"-DOCIO_PREFIX={prefix}"])


def required_tool(name):
    result = shutil.which(name)
    if result is None:
        raise FileNotFoundError(f"Required OCIO build tool missing: {name}")
    return str(Path(result).resolve())


def require_host(platform):
    system = {"win32": "windows", "linux": "linux", "darwin": "macos"}.get(sys.platform)
    arch = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "arm64", "aarch64": "arm64"}.get(host_platform.machine().lower())
    if platform != f"{system}-{arch}":
        raise ValueError("OCIO recipe requires a native build for the selected platform")


def publish(build, components, source):
    from package_desktop import verify_binary
    if any(digest(path) != expected for path, expected in build.identity.items()):
        raise ValueError("OCIO recipe, source lock or bridge changed during build")
    prefix = build.prefix
    licenses = prefix / "licenses" / "ocio"
    licenses.mkdir(parents=True)
    entries = []
    for item in components:
        original = source[item["name"]] / item["license_path"]
        if digest(original) != item["license_sha256"]:
            raise ValueError(f"OCIO dependency license checksum mismatch: {item['name']}")
        destination = licenses / (item["name"] + "-LICENSE.txt")
        shutil.copyfile(original, destination)
        entries.append(dict(path=destination.relative_to(prefix).as_posix(), sha256=digest(destination)))
    patterns = {"windows-x86_64": ("bin/OpenColorIO*.dll", "bin/motion_ocio_bridge.dll"),
                "linux-x86_64": ("lib/libOpenColorIO.so.*", "lib/libmotion_ocio_bridge.so")}.get(
                    build.platform, ("lib/libOpenColorIO*.dylib", "lib/libmotion_ocio_bridge.dylib"))
    files = []
    for library in sorted(library for pattern in patterns for library in prefix.glob(pattern)):
        # Include the versioned SONAME and its links in the verified payload.
        verify_binary(library, build.platform)
        files.append(dict(path=library.relative_to(prefix).as_posix(), sha256=digest(library)))
    if len(files) < 2:
        raise FileNotFoundError("Native OCIO shared runtime was not installed")
    manifest = dict(schemaVersion=1, platform=build.platform, version="2.6.0", recipeSha256=digest(__file__),
                    bridgeSourceSha256=digest(Path(__file__).with_name("ocio-native") / "bridge.cpp"),
                    sourceLockSha256=digest(LOCK), components=components, files=files, licenses=entries)
    # Publication is last. An incomplete directory cannot qualify as a runtime.
    with (prefix / "ocio-runtime.json").open("x", encoding="utf-8") as out:
        json.dump(manifest, out, indent=2)
        out.write("\n")
    return prefix / "ocio-runtime.json"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=PLATFORMS, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-cache", type=Path, required=True)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--cmake", default="cmake")
    parser.add_argument("--ninja", default="ninja")
    parser.add_argument("--jobs", type=int, choices=range(1, 65), default=2)
    args = parser.parse_args()
    require_host(args.platform)
    cmake, ninja = required_tool(args.cmake), required_tool(args.ninja)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    build = Build(output, args.platform, cmake, ninja, args.jobs)
    try:
        cache = args.source_cache.resolve()
        cache.mkdir(parents=True, exist_ok=True)
        folder = output / "sources"
        folder.mkdir()
        components = sources(args.platform)
        prepared = {item["name"]: extract(download(item, cache, args.offline), folder / item["name"]) for item in components}
        build.compile(prepared)
        manifest = publish(build, components, prepared)
    except BaseException as error:
        message = f"OCIO preparation failed type={type(error).__name__} cause={str(error)[:2048]}"
        build.event(level="error", operation="prepare", cause=message)
        print(message, file=sys.stderr)
        raise
    print(json.dumps(dict(manifest=str(manifest))))


if __name__ == "__main__":
    main()
