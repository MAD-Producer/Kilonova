"""One cancellable RGB/depth transform phase, without launching another process.

The host launches preparation, the native GPU worker, and remapping separately.
Cancellation therefore never leaves an unowned inference grandchild. The model
and the exact shared ONNX RGB/letterbox/align-corners procedure are unchanged.
"""
from pathlib import Path
import hashlib
import json
import math
import os
import sys
import time
import ai_depth_onnx as shared
from ai_depth_onnx import PROCESSING, SIDE, preprocess, resize_align_corners


def require(condition, cause):
    if not condition:
        raise ValueError(cause)


def identity(value):
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def manifest(directory):
    path = directory / "transform.json"
    require(path.stat().st_size <= 65536, "transform manifest exceeds 64 KiB")
    request = json.loads(path.read_text(encoding="utf-8"))
    require(set(request) == {"schema", "stage", "processing", "width", "height", "rgb_sha256", "depth_sha256"},
            "unexpected or missing transform fields")
    require(type(request["schema"]) is int and request["schema"] == 1 and request["processing"] == PROCESSING,
            "unsupported transform identity/version")
    require(request["stage"] in ("prepare", "remap"), "unsupported transform phase")
    for field in ("width", "height"):
        require(type(request[field]) is int and 0 < request[field] <= 4096, "invalid transform dimensions")
    require(identity(request["rgb_sha256"]), "invalid RGB digest")
    require(request["depth_sha256"] is None if request["stage"] == "prepare" else identity(request["depth_sha256"]),
            "invalid depth phase identity")
    return request


def owned_bytes(path, size, digest):
    with path.open("rb") as source:
        require(os.fstat(source.fileno()).st_size == size, "transform byte length mismatch")
        payload = source.read(size + 1)
    require(len(payload) == size and hashlib.sha256(payload).hexdigest() == digest, "transform digest/length mismatch")
    return payload


def execution_environment(np, cv2):
    plan = hashlib.sha256(b"MotionStudio/depth/wgsl-transform-plan/v1\0")
    for path in (Path(shared.__file__), Path(__file__)):
        # Only the two independently authored, embedded worker sources belong
        # to this plan; never read an unbounded caller-selected source tree.
        with path.open("rb") as source:
            payload = source.read((1 << 20) + 1)
        require(len(payload) <= 1 << 20, "transform source exceeds 1 MiB")
        plan.update(len(payload).to_bytes(8, "little"))
        plan.update(payload)
    return {"backend": "wgsl-transform", "provider": "NumPy/OpenCV", "precision": "float32",
            "runtime_sha256": shared.numerical_runtime_hash(
                (("numpy", np), ("cv2", cv2)), np, cv2,
                b"MotionStudio/depth/wgsl-transform-runtime/v1\0", {"processing": PROCESSING}),
            "plan_sha256": list(plan.digest())}


def resolve():
    # No source RGB, depth file, model, GPU operation or ORT import is needed
    # before the host decides whether a persistent raw result is reusable.
    import numpy as np
    import cv2
    return {"schema": 1, "stage": "resolve", "processing": PROCESSING,
            "execution": execution_environment(np, cv2), "encoded_operations": 0}


def expected_execution(directory):
    with (directory / "transform-execution.json").open("rb") as source:
        payload = source.read(65537)
    require(len(payload) <= 65536, "transform execution manifest exceeds 64 KiB")
    expected = json.loads(payload)
    require(isinstance(expected, dict) and set(expected) == {
        "backend", "provider", "precision", "runtime_sha256", "plan_sha256"},
        "unexpected or missing transform execution fields")
    for field in ("runtime_sha256", "plan_sha256"):
        require(isinstance(expected[field], list) and len(expected[field]) == 32
                and all(type(n) is int and 0 <= n <= 255 for n in expected[field]),
                "invalid transform execution digest")
    require(expected["backend"] == "wgsl-transform" and expected["provider"] == "NumPy/OpenCV"
            and expected["precision"] == "float32", "unsupported transform execution contract")
    return expected


def transform(directory, attest=False):
    began = time.perf_counter_ns()
    request = manifest(directory)
    width, height = request["width"], request["height"]
    import numpy as np
    execution = None
    if attest:
        import cv2
        expected = expected_execution(directory)
        execution = execution_environment(np, cv2)
        require(execution == expected, "transform execution changed since cache resolution")
    if request["stage"] == "prepare":
        import cv2
        raw = owned_bytes(directory / "input.rgb", width * height * 3, request["rgb_sha256"])
        rgb = np.frombuffer(raw, dtype=np.uint8).reshape(height, width, 3)
        tensor, _, _ = preprocess(rgb, width, height, np, cv2)
        output = tensor.astype("<f4", copy=False).tobytes(order="C")
        path = directory / "tensor.f32"
    else:
        raw = owned_bytes(directory / "depth518.f32", SIDE * SIDE * 4, request["depth_sha256"])
        plane = np.frombuffer(raw, dtype="<f4").reshape(SIDE, SIDE)
        require(bool(np.isfinite(plane).all()), "nonfinite native depth")
        # Same integer rectangle as the shared RGB procedure; no resampling of
        # RGB or learned computation is needed for this inverse mapping.
        scale = min(SIDE / width, SIDE / height)
        rw = max(1, min(SIDE, math.floor(width * scale + 0.5)))
        rh = max(1, min(SIDE, math.floor(height * scale + 0.5)))
        x, y = (SIDE - rw) // 2, (SIDE - rh) // 2
        values = resize_align_corners(plane[y:y+rh, x:x+rw], width, height, np)
        require(bool(np.isfinite(values).all()), "nonfinite remapped depth")
        output = values.astype("<f4", copy=False).tobytes(order="C")
        path = directory / "output.f32"
    path.write_bytes(output)
    report = {"schema": 1, "stage": request["stage"], "processing": PROCESSING,
            "width": width, "height": height, "rgb_sha256": request["rgb_sha256"],
            "depth_sha256": request["depth_sha256"], "output_sha256": hashlib.sha256(output).hexdigest(),
            "output_bytes": len(output), "elapsed_ns": time.perf_counter_ns() - began}
    if execution is not None:
        report["execution"] = execution
    return report


if __name__ == "__main__":
    protocol = os.dup(sys.stdout.fileno())
    os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
    try:
        args = sys.argv[1:]
        if args == ["--resolve"]:
            result = resolve()
        elif len(args) == 2 and args[0] == "--attest":
            result = transform(Path(args[1]).resolve(), attest=True)
        else:
            require(len(args) == 1 and not args[0].startswith("--"),
                    "usage: depth_tensor.py <directory> | --resolve | --attest <directory>")
            result = transform(Path(args[0]).resolve())
        os.write(protocol, (json.dumps(result, allow_nan=False) + "\n").encode("utf-8"))
    except Exception as error:
        os.write(protocol, (json.dumps({"schema": 1, "error": f"{type(error).__name__}: {str(error)[:1536]}"}) + "\n").encode("utf-8"))
        sys.exit(1)
    finally:
        os.close(protocol)
