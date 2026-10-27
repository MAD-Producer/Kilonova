"""Export externally installed, verified Small weights for the optional host.

No downloads or installation. A new, explicit output directory receives the
ONNX model, original license and provenance after a real CPU reference check.
"""
from pathlib import Path
import argparse
import hashlib
import importlib.metadata
import json
import os
import shutil
import subprocess
import sys
import tempfile

REFERENCE = "https://github.com/DepthAnything/Depth-Anything-V2"
REVISION = "a561b849ebae10a6f5ef49e26c83cbbcd36c71bf"
WEIGHTS_URL = "https://huggingface.co/depth-anything/Depth-Anything-V2-Small/resolve/main/depth_anything_v2_vits.pth"
WEIGHTS_SHA256 = "715fade13be8f229f8a70cc02066f656f2423a59effd0579197bbf57860e1378"
PROCESSING = "dav2-small-rgb8-letterbox518-align-corners-relative-v1"


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(4 << 20), b""):
            result.update(block)
    return result.hexdigest()


def export(reference, weights, destination):
    if any(not p.is_absolute() for p in (reference, weights, destination)):
        raise ValueError("reference, weights and output paths must be absolute")
    if destination.exists():
        raise FileExistsError("output already exists; choose a new directory")
    destination.parent.mkdir(parents=True, exist_ok=True)
    revision = subprocess.check_output(["git", "-C", str(reference), "rev-parse", "HEAD"], text=True).strip()
    if revision != REVISION:
        raise ValueError("reference checkout differs from the pinned revision")
    subprocess.run(["git", "-C", str(reference), "diff", "--quiet", "HEAD", "--"], check=True)
    if weights.stat().st_size != 99218434 or digest(weights) != WEIGHTS_SHA256:
        raise ValueError("Small weights differ from the pinned size or SHA-256")
    with tempfile.TemporaryDirectory(prefix="motion-depth-export-", dir=destination.parent) as temporary:
        scratch = Path(temporary)
        for name in ("TEMP", "TMP", "TORCH_HOME", "HF_HOME", "TORCHINDUCTOR_CACHE_DIR"):
            directory = scratch / name.lower()
            directory.mkdir()
            os.environ[name] = str(directory)
        os.environ["CUDA_VISIBLE_DEVICES"] = ""
        sys.dont_write_bytecode = True
        sys.path.insert(0, str(reference))
        import numpy as np
        import onnx
        import onnxruntime as ort
        import torch
        from depth_anything_v2.dpt import DepthAnythingV2

        torch.set_num_threads(1)
        torch.set_num_interop_threads(1)
        model = DepthAnythingV2(encoder="vits", features=64, out_channels=[48, 96, 192, 384]).cpu().eval()
        model.load_state_dict(torch.load(weights, map_location="cpu", weights_only=True), strict=True)
        y, x = np.indices((109, 109))
        bgr = np.stack((x * 255 // 108, y * 255 // 108, (x + y) * 255 // 216), axis=-1).astype(np.uint8)
        tensor, _ = model.image2tensor(bgr, input_size=518)
        if tuple(tensor.shape) != (1, 3, 518, 518):
            raise ValueError("official preprocessing returned an unexpected shape")
        staged = scratch / "accepted"
        staged.mkdir()
        output = staged / "depth_anything_v2_vits_518.onnx"
        with torch.no_grad():
            expected = model(tensor).cpu().numpy()
            torch.onnx.export(model, (tensor,), str(output), input_names=["rgb"],
                              output_names=["relative_inverse_depth"], opset_version=17,
                              dynamo=False, external_data=False)
        if output.stat().st_size > 256 << 20:
            raise ValueError("ONNX export exceeds the host model budget")
        onnx.checker.check_model(str(output))
        options = ort.SessionOptions()
        options.intra_op_num_threads = options.inter_op_num_threads = 1
        session = ort.InferenceSession(str(output), sess_options=options, providers=["CPUExecutionProvider"])
        session.disable_fallback()
        actual = session.run(["relative_inverse_depth"], {"rgb": tensor.numpy().copy(order="C")})[0]
        if actual.shape != (1, 518, 518) or not np.isfinite(actual).all():
            raise ValueError("ONNX returned invalid scalar depth")
        error = np.abs(actual.astype(np.float64) - expected.astype(np.float64))
        amplitude = max(float(np.abs(expected).max()), 1e-6)
        maximum, mean = float(error.max()) / amplitude, float(error.mean()) / amplitude
        if maximum > 2e-4 or mean > 2e-5:
            raise ValueError(f"official reference differs: max={maximum}, mean={mean}")
        shutil.copyfile(reference / "LICENSE", staged / "LICENSE-Depth-Anything-V2.txt")
        manifest = {
            "schema": 1, "id": "depth-anything-v2-small", "source": REFERENCE,
            "revision": REVISION, "weightsSource": WEIGHTS_URL,
            "weightsSha256": WEIGHTS_SHA256, "license": "Apache-2.0 (Small only)",
            "modification": "Unmodified reference; fixed-shape CPU ONNX opset-17 export",
            "file": output.name, "sha256": digest(output), "bytes": output.stat().st_size,
            "processing": PROCESSING, "measure": "relative", "near": "increasing",
            "versions": {name: importlib.metadata.version(name) for name in
                         ("torch", "onnx", "onnxruntime", "numpy", "opencv-python-headless")},
            "referenceProbe": {"maxNormalized": maximum, "meanNormalized": mean,
                               "maxBound": 2e-4, "meanBound": 2e-5},
            "scope": "external optional prototype; no metric calibration or quality guarantee",
        }
        (staged / "model.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        del session, model
        staged.rename(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--weights", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    export(args.reference, args.weights, args.output)
