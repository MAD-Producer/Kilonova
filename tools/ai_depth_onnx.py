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


def elapsed(start):
    return time.perf_counter_ns() - start


def preprocess(rgb, width, height, np, cv2):
    scale = min(SIDE / width, SIDE / height)
    rw = max(1, min(SIDE, math.floor(width * scale + 0.5)))
    rh = max(1, min(SIDE, math.floor(height * scale + 0.5)))
    x = (SIDE - rw) // 2
    y = (SIDE - rh) // 2
    image = np.zeros((SIDE, SIDE, 3), dtype=np.uint8)
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


def infer(directory):
    phase = "manifest"
    began = time.perf_counter_ns()
    try:
        path = directory / "request.json"
        if path.stat().st_size > 65536:
            raise ValueError("manifest exceeds 64 KiB")
        request = json.loads(path.read_text(encoding="utf-8"))
        if set(request) != {"schema", "model", "sha256", "processing", "width", "height", "provider"}:
            raise ValueError("unexpected or missing request fields")
        if request["schema"] != SCHEMA or request["processing"] != PROCESSING:
            raise ValueError("unsupported depth inference procedure")
        width, height = request["width"], request["height"]
        if type(width) is not int or type(height) is not int or not (0 < width <= 4096 and 0 < height <= 4096):
            raise ValueError("dimensions exceed depth input budget")
        source = directory / "input.rgb"
        if source.stat().st_size != width * height * 3:
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
        if input_meta.shape != [1, 3, SIDE, SIDE] or input_meta.type != "tensor(float)":
            raise ValueError("model input differs from fixed 1x3x518x518 float32 contract")
        if output_meta.shape != [1, SIDE, SIDE] or output_meta.type != "tensor(float)":
            raise ValueError("model output differs from fixed 1x518x518 float32 contract")
        load_ns = elapsed(began)
        phase = "preprocess"
        started = time.perf_counter_ns()
        rgb = np.fromfile(source, dtype=np.uint8).reshape(height, width, 3)
        tensor, _, rectangle = preprocess(rgb, width, height, np, cv2)
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
        if depth.shape != (1, SIDE, SIDE) or not np.isfinite(depth).all():
            raise ValueError("runtime returned invalid scalar depth")
        phase = "postprocess"
        started = time.perf_counter_ns()
        x, y, rw, rh = rectangle
        output = resize_align_corners(depth[0, y:y+rh, x:x+rw], width, height, np)
        if not np.isfinite(output).all():
            raise ValueError("postprocessing returned nonfinite depth")
        output.astype("<f4", copy=False).tofile(directory / "output.f32")
        postprocess_ns = elapsed(started)
        return {"schema": SCHEMA, "sha256": expected, "processing": PROCESSING,
                "width": width, "height": height, "providers": actual,
                "requested_provider": requested, "available_providers": available,
                "cpu_fallback": requested != "CPUExecutionProvider" and requested not in actual,
                "runtime_version": ort.__version__, "letterbox": list(rectangle),
                "costs_ns": {"load": load_ns, "preprocess": preprocess_ns,
                    "input_copy": input_copy_ns, "inference": inference_ns,
                    "output_copy": output_copy_ns, "postprocess": postprocess_ns},
                "input_host_copy_bytes": int(bound.nbytes), "output_host_copy_bytes": int(depth.nbytes),
                "resource_bytes": output.size * 4,
                "transfer_scope": "host copies measured; provider-internal transfers included in inference; no IO binding"}
    except Exception as error:
        raise RuntimeError(f"phase={phase} type={type(error).__name__} cause={str(error)[:1536]}") from error


if __name__ == "__main__":
    # Third-party Python/native initialization may print diagnostics to stdout.
    # Preserve a separate protocol descriptor, and redirect those diagnostics.
    protocol = os.dup(sys.stdout.fileno())
    os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
    try:
        os.write(protocol, (json.dumps(infer(Path(sys.argv[1]).resolve()), allow_nan=False)+"\n").encode("utf-8"))
    except Exception as error:
        os.write(protocol, (json.dumps({"schema": SCHEMA, "error": str(error)[:2048]}, allow_nan=False)+"\n").encode("utf-8"))
        sys.exit(1)
    finally:
        os.close(protocol)
