"""Bounded file protocol for a fixed-shape, explicitly letterboxed depth model.

Independently authored host procedure. Weights and ONNX Runtime are separately
installed. No network, synthetic depth, implicit metric calibration or zero-copy
claim. Profiling determines providers that actually executed graph nodes.
"""
from pathlib import Path
import hashlib
import json
import math
import os
import sys
import time

SCHEMA = 1
SIDE = 518
PROCESSING = "dav2-small-rgb8-letterbox518-align-corners-relative-v1"


def processing_side(identity):
    # Processing identity is part of the resource/cache key. Never infer it
    # from a filename, provider choice or the dimensions of the source image.
    for side in (266, 392, 518):
        if identity == f"dav2-small-rgb8-letterbox{side}-align-corners-relative-v1":
            return side
    raise ValueError("unsupported depth inference procedure")


def elapsed(start):
    return time.perf_counter_ns() - start


def _resident_peak_bytes(value, system):
    # Linux ru_maxrss is KiB; Darwin's current kernel returns bytes, despite
    # the older archived manual. Never guess units for another operating system.
    multiplier = {"linux": 1024, "darwin": 1}.get(system)
    if multiplier is None or type(value) is not int or not 0 < value <= ((1 << 64) - 1) // multiplier:
        raise ValueError("unsupported or invalid process resident peak")
    return value * multiplier


def _process_peak():
    if sys.platform == "win32":
        import ctypes
        from ctypes import wintypes

        class Counters(ctypes.Structure):
            _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
                (name, ctypes.c_size_t) for name in (
                    "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                    "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage",
                    "PagefileUsage", "PeakPagefileUsage", "PrivateUsage")]

        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.GetCurrentProcess.argtypes = []
        kernel.GetCurrentProcess.restype = wintypes.HANDLE
        query = ctypes.WinDLL("psapi", use_last_error=True).GetProcessMemoryInfo
        query.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
        query.restype = wintypes.BOOL
        counters = Counters()
        counters.cb = ctypes.sizeof(counters)
        if not query(kernel.GetCurrentProcess(), ctypes.byref(counters), counters.cb):
            raise ctypes.WinError(ctypes.get_last_error())
        resident, private = counters.PeakWorkingSetSize, counters.PeakPagefileUsage
        if resident <= 0 or private <= 0:
            raise ValueError("process peak counters unavailable")
        return "windows_process_memory_counters", resident, private
    if sys.platform in ("linux", "darwin"):
        import resource
        resident = _resident_peak_bytes(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss, sys.platform)
        return "getrusage_" + sys.platform, resident, None
    raise ValueError("process peak query unsupported on this operating system")


def process_peak():
    """Own-process OS high-water marks, observed after output preparation.

    Includes runtime startup/model loading and retained Python allocations;
    excludes the parent, sibling/child processes and physical device memory.
    Query failure is observable metadata, not permission to fail inference or
    fabricate zero usage. No process enumeration, sampling or file IO is used.
    """
    try:
        method, resident, private = _process_peak()
        return dict(status="observed", pid=os.getpid(), method=method,
                    peak_resident_bytes=resident, peak_private_commit_bytes=private)
    except Exception as error:
        cause = f"{type(error).__name__}: {error}"
        cause = ''.join(c if c.isprintable() else ' ' for c in cause)[:512]
        return dict(status="unavailable", pid=os.getpid(), cause=cause)


def letterbox_rectangle(width, height, side=SIDE):
    if type(side) is not int or side not in (266, 392, 518):
        raise ValueError("unsupported depth preprocessing resolution")
    if any(type(n) is not int or not 0 < n <= 4096 for n in (width, height)):
        raise ValueError("dimensions exceed depth input budget")
    scale = min(side / width, side / height)
    rw = max(1, min(side, math.floor(width * scale + 0.5)))
    rh = max(1, min(side, math.floor(height * scale + 0.5)))
    x = (side - rw) // 2
    y = (side - rh) // 2
    return x, y, rw, rh


def preprocess(rgb, width, height, np, cv2, side=SIDE):
    x, y, rw, rh = letterbox_rectangle(width, height, side)
    image = np.zeros((side, side, 3), dtype=np.uint8)
    image[y:y+rh, x:x+rw] = cv2.resize(rgb, (rw, rh), interpolation=cv2.INTER_CUBIC)
    mean = np.array([0.485, 0.456, 0.406], dtype=np.float64)
    std = np.array([0.229, 0.224, 0.225], dtype=np.float64)
    tensor = ((image.astype(np.float64) / 255.0 - mean) / std).astype(np.float32).transpose(2, 0, 1)[None]
    return tensor, image, (x, y, rw, rh)


def resize_align_corners(values, width, height, np):
    """One scalar plane, pixel-centre endpoints matching align_corners=True."""
    ih, iw = values.shape
    x = np.linspace(0, iw - 1, width, dtype=np.float32) if width > 1 else np.zeros(1, np.float32)
    y = np.linspace(0, ih - 1, height, dtype=np.float32) if height > 1 else np.zeros(1, np.float32)
    x0, y0 = np.floor(x).astype(np.int32), np.floor(y).astype(np.int32)
    x1, y1 = np.minimum(x0 + 1, iw - 1), np.minimum(y0 + 1, ih - 1)
    tx, ty = (x - x0).astype(np.float32), (y - y0).astype(np.float32)
    # Rows bound intermediates to output_width instead of retaining four full
    # 4096-square temporary planes. The returned resource is one float32 plane.
    output = np.empty((height, width), dtype=np.float32)
    for row in range(height):
        top = values[y0[row], x0] * (1 - tx) + values[y0[row], x1] * tx
        bottom = values[y1[row], x0] * (1 - tx) + values[y1[row], x1] * tx
        output[row] = top * (1 - ty[row]) + bottom * ty[row]
    return output


def numerical_runtime_hash(packages, np, cv2, domain, extra_context):
    """Bound the installed numerical code without requiring an ONNX runtime.

    Callers select their actual packages, domain and execution-specific build
    information. The WGSL RGB/remap child must not acquire an ORT dependency
    merely to identify NumPy/OpenCV. This is an observed identity, not a lease
    against concurrent replacement of installed executable files.
    """
    import platform
    digest = hashlib.sha256(domain)
    total = 0
    count = 0
    visited = 0
    roots = [(name, Path(module.__file__).resolve().parent) for name, module in packages]
    sites = sorted({root.parent for _, root in roots})
    prefixes = tuple("opencv" if name == "cv2" else name for name, _ in packages)
    # Wheels keep BLAS/OpenCV dependencies in sibling .libs directories on
    # Unix. Those bytes affect numerical execution just as the extension does.
    entries = 0
    libraries = []
    for site in sites:
        for root in site.iterdir():
            entries += 1
            if entries > 8192:
                raise ValueError("runtime site-packages enumeration exceeds entry bound")
            if root.name.endswith(".libs") and root.name.startswith(prefixes):
                if root.is_symlink():
                    raise ValueError("linked runtime dependency directory rejected")
                libraries.append((root.name, root))
    # Filesystem enumeration order must not cause a different cache identity.
    roots.extend(sorted(libraries))
    for name, root in roots:
        # Sort only a bounded list and do not admit links as executable identity.
        paths = []
        for parent, dirs, files in os.walk(root, followlinks=False):
            visited += 1 + len(files)
            if visited > 8192:
                raise ValueError("runtime package traversal exceeds entry bound")
            if len(dirs) > 4096 or any((Path(parent) / d).is_symlink() for d in dirs):
                raise ValueError("runtime package directory bound or link rejected")
            dirs[:] = [d for d in dirs if d != "__pycache__"]
            for file in files:
                path = Path(parent) / file
                if path.suffix.lower() not in {".py", ".pyd", ".dll", ".so", ".dylib"} and ".so." not in file:
                    continue
                count += 1
                if count > 4096 or path.is_symlink():
                    raise ValueError("runtime package file bound or link rejected")
                paths.append(path)
        for path in sorted(paths):
            digest.update((name + "/" + path.relative_to(root).as_posix()).encode("utf-8") + b"\0")
            size = path.stat().st_size
            total += size
            if total > 512 << 20:
                raise ValueError("runtime attestation exceeds 512 MiB")
            digest.update(size.to_bytes(8, "little"))
            with path.open("rb") as stream:
                remaining = size
                while remaining:
                    chunk = stream.read(min(65536, remaining))
                    if not chunk:
                        raise ValueError("runtime package changed during attestation")
                    digest.update(chunk)
                    remaining -= len(chunk)
                if stream.read(1):
                    raise ValueError("runtime package grew during attestation")
    environment = {k: v for k, v in os.environ.items()
                   if k.startswith(("OMP_", "MKL_", "OPENBLAS_", "NPY_", "ORT_"))}
    if len(environment) > 64 or any(len(k) > 128 or len(v) > 1024 for k, v in environment.items()):
        raise ValueError("unbounded numerical environment")
    features = getattr(np._core._multiarray_umath, "__cpu_features__", {})
    context = {"python": sys.version, "machine": platform.machine(), "system": platform.system(),
               "release": platform.release(), "cpu_features": features, "environment": environment,
               "cv_build": cv2.getBuildInformation(), "execution": extra_context}
    encoded_context = json.dumps(context, sort_keys=True, allow_nan=False).encode("utf-8")
    if len(encoded_context) > 65536:
        raise ValueError("numerical runtime context exceeds 64 KiB")
    digest.update(encoded_context)
    return list(digest.digest())


def execution_environment(ort, np, cv2):
    """CPU-only attestation, before lookup and again in the executing child.

    Available accelerator providers do not identify actual node assignment.
    Refuse to attest those until a dedicated execution contract exists.
    """
    return {"backend": "onnx", "provider": "CPUExecutionProvider", "precision": "float32",
            "runtime_sha256": numerical_runtime_hash(
                (("onnxruntime", ort), ("numpy", np), ("cv2", cv2)), np, cv2,
                b"MotionStudio/depth/cpu-runtime/v2\0", {"ort_build": ort.get_build_info()}),
            "plan_sha256": list(hashlib.sha256(Path(__file__).read_bytes()).digest())}


def infer(directory, resolve_only=False, attest=False):
    phase = "manifest"
    began = time.perf_counter_ns()
    try:
        path = directory / "request.json"
        if path.stat().st_size > 65536:
            raise ValueError("manifest exceeds 64 KiB")
        request = json.loads(path.read_text(encoding="utf-8"))
        if set(request) != {"schema", "model", "sha256", "processing", "width", "height", "provider"}:
            raise ValueError("unexpected or missing request fields")
        if type(request["schema"]) is not int or request["schema"] != SCHEMA:
            raise ValueError("unsupported depth inference procedure")
        processing = request["processing"]
        side = processing_side(processing)
        width, height = request["width"], request["height"]
        if type(width) is not int or type(height) is not int or not (0 < width <= 4096 and 0 < height <= 4096):
            raise ValueError("dimensions exceed depth input budget")
        source = directory / "input.rgb"
        if not resolve_only and source.stat().st_size != width * height * 3:
            raise ValueError("RGB input byte count differs from dimensions")
        expected = request["sha256"]
        if not isinstance(expected, str) or len(expected) != 64 or any(c not in "0123456789abcdef" for c in expected):
            raise ValueError("invalid expected model digest")
        phase = "model_hash"
        model_path = Path(request["model"])
        if not model_path.is_absolute() or not (0 < model_path.stat().st_size <= 256 << 20):
            raise ValueError("model must be a nonempty absolute file, up to 256 MiB")
        with model_path.open("rb") as model:
            # Hash and execute the same owned bytes. Loading the path again
            # would race a concurrent model import/replacement after hashing.
            model_bytes = model.read((256 << 20) + 1)
        if not (0 < len(model_bytes) <= 256 << 20):
            raise ValueError("model changed size or exceeds the model byte budget")
        if hashlib.sha256(model_bytes).hexdigest() != expected:
            raise ValueError("ONNX model SHA-256 differs from requested identity")
        phase = "runtime_import"
        import cv2
        import numpy as np
        import onnxruntime as ort
        requested = request["provider"]
        if requested not in {"CPUExecutionProvider", "CUDAExecutionProvider", "DmlExecutionProvider", "CoreMLExecutionProvider"}:
            raise ValueError("unsupported requested execution provider")
        available = ort.get_available_providers()
        if "CPUExecutionProvider" not in available:
            raise ValueError("runtime has no CPU execution provider")
        providers = [requested, "CPUExecutionProvider"] if requested in available and requested != "CPUExecutionProvider" else ["CPUExecutionProvider"]
        phase = "session"
        options = ort.SessionOptions()
        options.intra_op_num_threads = 1
        options.inter_op_num_threads = 1
        options.enable_profiling = True
        options.profile_file_prefix = str(directory / "ort-profile")
        options.log_severity_level = 3
        # Unavailable requested providers use CPU directly. ORT may internally
        # fall back during construction; actual profiled providers and captured
        # runtime diagnostics expose that event. Do not retry session.run().
        session = ort.InferenceSession(model_bytes, sess_options=options, providers=providers)
        del model_bytes
        session.disable_fallback()
        if len(session.get_inputs()) != 1 or len(session.get_outputs()) != 1:
            raise ValueError("expected exactly one image input and depth output")
        input_meta, output_meta = session.get_inputs()[0], session.get_outputs()[0]
        if input_meta.shape != [1, 3, side, side] or input_meta.type != "tensor(float)":
            raise ValueError(f"model input differs from requested 1x3x{side}x{side} float32 contract")
        if output_meta.shape != [1, side, side] or output_meta.type != "tensor(float)":
            raise ValueError(f"model output differs from requested 1x{side}x{side} float32 contract")
        execution = None
        if resolve_only or attest:
            phase = "execution_attestation"
            if requested != "CPUExecutionProvider" or session.get_providers() != ["CPUExecutionProvider"]:
                raise ValueError("persistent caching requires an exclusive actual CPU session")
            execution = execution_environment(ort, np, cv2)
            if resolve_only:
                return {"schema": SCHEMA, "execution": execution, "process_memory": process_peak()}
        load_ns = elapsed(began)
        phase = "preprocess"
        started = time.perf_counter_ns()
        rgb = np.fromfile(source, dtype=np.uint8).reshape(height, width, 3)
        tensor, _, rectangle = preprocess(rgb, width, height, np, cv2, side)
        preprocess_ns = elapsed(started)
        phase = "input_host_copy"
        started = time.perf_counter_ns()
        bound = tensor.copy(order="C")
        input_copy_ns = elapsed(started)
        phase = "inference"
        started = time.perf_counter_ns()
        result = session.run([output_meta.name], {input_meta.name: bound})[0]
        inference_ns = elapsed(started)
        profile_path = Path(session.end_profiling())
        if profile_path.stat().st_size > 16 << 20:
            raise ValueError("execution-provider profile exceeds budget")
        events = json.loads(profile_path.read_text(encoding="utf-8"))
        actual = sorted({e.get("args", {}).get("provider") for e in events if e.get("cat") == "Node" and e.get("args", {}).get("provider")})
        if not actual or any(p not in available for p in actual):
            raise ValueError("runtime profile does not identify actual node execution providers")
        phase = "output_host_copy"
        started = time.perf_counter_ns()
        depth = result.astype(np.float32, copy=True)
        output_copy_ns = elapsed(started)
        if depth.shape != (1, side, side) or not np.isfinite(depth).all():
            raise ValueError("runtime returned invalid scalar depth")
        phase = "postprocess"
        started = time.perf_counter_ns()
        x, y, rw, rh = rectangle
        output = resize_align_corners(depth[0, y:y+rh, x:x+rw], width, height, np)
        if not np.isfinite(output).all():
            raise ValueError("postprocessing returned nonfinite depth")
        output.astype("<f4", copy=False).tofile(directory / "output.f32")
        postprocess_ns = elapsed(started)
        return {"schema": SCHEMA, "sha256": expected, "processing": processing, "execution": execution,
                "width": width, "height": height, "providers": actual,
                "requested_provider": requested, "available_providers": available,
                "cpu_fallback": requested != "CPUExecutionProvider" and requested not in actual,
                "runtime_version": ort.__version__, "letterbox": list(rectangle),
                "costs_ns": {"load": load_ns, "preprocess": preprocess_ns,
                    "input_copy": input_copy_ns, "inference": inference_ns,
                    "output_copy": output_copy_ns, "postprocess": postprocess_ns},
                "input_host_copy_bytes": int(bound.nbytes), "output_host_copy_bytes": int(depth.nbytes),
                "resource_bytes": output.size * 4,
                "inference_process_memory": process_peak(),
                "transfer_scope": "host copies measured; provider-internal transfers included in inference; no IO binding"}
    except Exception as error:
        raise RuntimeError(f"phase={phase} type={type(error).__name__} cause={str(error)[:1536]}") from error


if __name__ == "__main__":
    # Third-party Python/native initialization may print diagnostics to stdout.
    # Preserve a separate protocol descriptor, and redirect those diagnostics.
    protocol = os.dup(sys.stdout.fileno())
    os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
    try:
        if len(sys.argv) not in (2, 3) or (len(sys.argv) == 3 and sys.argv[2] not in {"--resolve", "--attest"}):
            raise ValueError("unsupported depth worker invocation")
        mode = sys.argv[2] if len(sys.argv) == 3 else ""
        os.write(protocol, (json.dumps(infer(Path(sys.argv[1]).resolve(), mode == "--resolve", mode == "--attest"), allow_nan=False)+"\n").encode("utf-8"))
    except Exception as error:
        os.write(protocol, (json.dumps({"schema": SCHEMA, "error": str(error)[:2048]}, allow_nan=False)+"\n").encode("utf-8"))
        sys.exit(1)
    finally:
        os.close(protocol)
