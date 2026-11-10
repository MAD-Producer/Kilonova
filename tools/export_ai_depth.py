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
SIDES = (266, 392, 518)
METRIC_WEIGHTS_REVISION = "3bc65d4e14a6786a61acec16453c50e12bf5f338"
METRIC_WEIGHTS_URL = ("https://huggingface.co/depth-anything/Depth-Anything-V2-Metric-Hypersim-Small/resolve/"
                      + METRIC_WEIGHTS_REVISION + "/depth_anything_v2_metric_hypersim_vits.pth")
METRIC_WEIGHTS_SHA256 = "b782898d8a3e8be1f639de33837ed85e9b4b73e40f8f5e5cd99067588d722545"
METRIC_PROCESSING = "dav2-metric-hypersim-small-rgb8-letterbox518-align-corners-camera-z-metres-v1"
METRIC_NATIVE_PROCESSING = "dav2-metric-hypersim-small-rgb8-official686x518-align-corners-camera-z-metres-v1"


def processing_identity(side, *, metric=False, native_aspect=False):
    if type(metric) is not bool or type(native_aspect) is not bool:
        raise ValueError("metric profile flags must be boolean")
    if native_aspect and not metric:
        raise ValueError("native-aspect export requires the explicit metric profile")
    if metric:
        if type(side) is not int or side != 518:
            raise ValueError("metric export side must be 518 until independently evaluated")
        return METRIC_NATIVE_PROCESSING if native_aspect else METRIC_PROCESSING
    # Candidates for measurement, not calibrated fast/standard/high tiers.
    # Refuse coercion: a bool or float must not identify another model profile.
    if type(side) is not int or side not in SIDES:
        raise ValueError("export side must be one of 266, 392, 518")
    return f"dav2-small-rgb8-letterbox{side}-align-corners-relative-v1"


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(4 << 20), b""):
            result.update(block)
    return result.hexdigest()


def preprocess_metric_native(rgb, np, cv2):
    """The explicit 640x480 research profile; no implicit aspect coercion.

    The pinned official path scales RGB to Float64 before cubic resizing.
    Resizing UInt8 first introduces a different rounding operation.
    """
    if rgb.shape != (480, 640, 3) or rgb.dtype != np.uint8:
        raise ValueError("native metric profile requires RGB8 640x480")
    resized = cv2.resize(rgb.astype(np.float64) / 255., (686, 518), interpolation=cv2.INTER_CUBIC)
    normalized = (resized - np.array([.485, .456, .406])) / np.array([.229, .224, .225])
    return np.ascontiguousarray(normalized.astype(np.float32).transpose(2, 0, 1)[None])


def export(reference, weights, destination, side=518, *, metric=False, native_aspect=False, observe=None):
    processing = processing_identity(side, metric=metric, native_aspect=native_aspect)
    output_width, output_height = (686, 518) if native_aspect else (side, side)
    def phase(name):
        if observe is not None:
            observe(name)
    weight_sha = METRIC_WEIGHTS_SHA256 if metric else WEIGHTS_SHA256
    weight_size = 99222290 if metric else 99218434
    output_name = "camera_z_metres" if metric else "relative_inverse_depth"
    if any(not p.is_absolute() for p in (reference, weights, destination)):
        raise ValueError("reference, weights and output paths must be absolute")
    if destination.exists():
        raise FileExistsError("output already exists; choose a new directory")
    destination.parent.mkdir(parents=True, exist_ok=True)
    phase("verify_reference_and_weights")
    revision = subprocess.check_output(["git", "-C", str(reference), "rev-parse", "HEAD"], text=True).strip()
    if revision != REVISION:
        raise ValueError("reference checkout differs from the pinned revision")
    subprocess.run(["git", "-C", str(reference), "diff", "--quiet", "HEAD", "--"], check=True)
    if weights.stat().st_size != weight_size or digest(weights) != weight_sha:
        raise ValueError("Small weights differ from the pinned size or SHA-256")
    with tempfile.TemporaryDirectory(prefix="motion-depth-export-", dir=destination.parent) as temporary:
        scratch = Path(temporary)
        for name in ("TEMP", "TMP", "TORCH_HOME", "HF_HOME", "TORCHINDUCTOR_CACHE_DIR"):
            directory = scratch / name.lower()
            directory.mkdir()
            os.environ[name] = str(directory)
        os.environ["CUDA_VISIBLE_DEVICES"] = ""
        sys.dont_write_bytecode = True
        phase("import_cpu_export_runtime")
        sys.path.insert(0, str(reference / "metric_depth" if metric else reference))
        import numpy as np
        import onnx
        import onnxruntime as ort
        import torch
        from depth_anything_v2.dpt import DepthAnythingV2

        torch.set_num_threads(1)
        torch.set_num_interop_threads(1)
        phase("load_verified_weights")
        configuration = dict(encoder="vits", features=64, out_channels=[48, 96, 192, 384])
        if metric:
            configuration["max_depth"] = 20.
        model = DepthAnythingV2(**configuration).cpu().eval()
        model.load_state_dict(torch.load(weights, map_location="cpu", weights_only=True), strict=True)
        probe_width, probe_height = (640, 480) if native_aspect else (109, 109)
        y, x = np.indices((probe_height, probe_width))
        bgr = np.stack((x * 255 // (probe_width-1), y * 255 // (probe_height-1),
                        (x+y) * 255 // (probe_width+probe_height-2)), axis=-1).astype(np.uint8)
        tensor, _ = model.image2tensor(bgr, input_size=side)
        if tuple(tensor.shape) != (1, 3, output_height, output_width):
            raise ValueError("official preprocessing returned an unexpected shape")
        staged = scratch / "accepted"
        staged.mkdir()
        output = staged / (f"depth_anything_v2_metric_hypersim_vits_{side}.onnx" if metric else f"depth_anything_v2_vits_{side}.onnx")
        if native_aspect:
            output = staged / "depth_anything_v2_metric_hypersim_vits_native686x518.onnx"
        phase("reference_cpu_probe_and_export")
        with torch.no_grad():
            phase("reference_cpu_probe")
            expected = model(tensor).cpu().numpy()
            if expected.shape != (1, output_height, output_width) or not np.isfinite(expected).all():
                raise ValueError("reference returned invalid scalar depth")
            phase("onnx_export")
            torch.onnx.export(model, (tensor,), str(output), input_names=["rgb"],
                              output_names=[output_name], opset_version=17,
                              dynamo=False, external_data=False)
        if output.stat().st_size > 256 << 20:
            raise ValueError("ONNX export exceeds the host model budget")
        phase("check_onnx_and_compare_cpu_probe")
        phase("onnx_check")
        onnx.checker.check_model(str(output))
        options = ort.SessionOptions()
        options.intra_op_num_threads = options.inter_op_num_threads = 1
        phase("onnx_cpu_session")
        session = ort.InferenceSession(str(output), sess_options=options, providers=["CPUExecutionProvider"])
        session.disable_fallback()
        inputs, outputs = session.get_inputs(), session.get_outputs()
        if (len(inputs) != 1 or len(outputs) != 1
                or inputs[0].name != "rgb" or outputs[0].name != output_name
                or inputs[0].shape != [1, 3, output_height, output_width] or outputs[0].shape != [1, output_height, output_width]
                or inputs[0].type != "tensor(float)" or outputs[0].type != "tensor(float)"
                or session.get_providers() != ["CPUExecutionProvider"]):
            raise ValueError("exported model metadata or provider differs from requested profile")
        phase("onnx_cpu_probe")
        actual = session.run([output_name], {"rgb": tensor.numpy().copy(order="C")})[0]
        if actual.shape != (1, output_height, output_width) or not np.isfinite(actual).all():
            raise ValueError("ONNX returned invalid scalar depth")
        if metric and (float(expected.min()) < 0. or float(expected.max()) > 20.
                       or float(actual.min()) < 0. or float(actual.max()) > 20.):
            raise ValueError("metric model exceeds its declared zero-to-20 metre output range")
        error = np.abs(actual.astype(np.float64) - expected.astype(np.float64))
        amplitude = max(float(np.abs(expected).max()), 1e-6)
        maximum, mean = float(error.max()) / amplitude, float(error.mean()) / amplitude
        if maximum > 2e-4 or mean > 2e-5:
            raise ValueError(f"official reference differs: max={maximum}, mean={mean}")
        shutil.copyfile(reference / "LICENSE", staged / "LICENSE-Depth-Anything-V2.txt")
        manifest = {
            "schema": 1, "id": "depth-anything-v2-metric-hypersim-small" if metric else "depth-anything-v2-small", "source": REFERENCE,
            "revision": REVISION, "weightsSource": METRIC_WEIGHTS_URL if metric else WEIGHTS_URL,
            "weightsSha256": weight_sha, "license": "Apache-2.0 (Small only)",
            "modification": "Unmodified reference; fixed-shape CPU ONNX opset-17 export",
            "file": output.name, "sha256": digest(output), "bytes": output.stat().st_size,
            "processing": processing, "inputShape": [1, 3, output_height, output_width],
            "outputShape": [1, output_height, output_width], "measure": "relative", "near": "increasing",
            "versions": {name: importlib.metadata.version(name) for name in
                         ("torch", "onnx", "onnxruntime", "numpy", "opencv-python-headless")},
            "referenceProbe": {"maxNormalized": maximum, "meanNormalized": mean,
                               "maxBound": 2e-4, "meanBound": 2e-5},
            "scope": "external optional resolution candidate; no host admission, quality tier, metric calibration or quality guarantee",
        }
        if metric:
            manifest.pop("near")
            manifest.update(measure="metric", axis="camera_z", metersPerUnit=1., maximumMetres=20.,
                            weightsRevision=METRIC_WEIGHTS_REVISION, outputName=output_name,
                            scope="external indoor metric research candidate; numerical export only, no sensor/domain or quality-tier qualification")
        if native_aspect:
            manifest.update(sourceExtent=[640, 480], preprocessing="Pinned official image2tensor on RGB8/BGR-converted 640x480; no letterbox or crop")
        phase("publish_verified_external_export")
        (staged / "model.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        del session, model
        staged.rename(destination)


def arguments(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--weights", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--side", type=int, choices=SIDES, default=518,
                        help="fixed square resolution candidate; not a measured quality tier")
    parser.add_argument("--metric-hypersim", action="store_true", help="explicit pinned indoor metric Small candidate; 518 only")
    parser.add_argument("--metric-native-aspect", action="store_true",
                        help="explicit metric research profile: official 640x480 preprocessing to 686x518; no quality qualification")
    return parser.parse_args(argv)


if __name__ == "__main__":
    args = arguments()
    if not args.metric_hypersim:
        export(args.reference, args.weights, args.output, args.side, native_aspect=args.metric_native_aspect)
    else:
        import logging
        import time
        logger = logging.getLogger("motion.metric_export")
        logger.setLevel(logging.INFO)
        # Invalid profiles must refuse before creating output or importing a
        # third-party runtime. The original CLI exception still reaches stderr.
        processing_identity(args.side, metric=True, native_aspect=args.metric_native_aspect)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        sink = logging.FileHandler(args.output.parent / (args.output.name + ".log"), encoding="utf-8")
        sink.setFormatter(logging.Formatter("%(asctime)s %(levelname)s %(message)s"))
        errors = logging.StreamHandler(sys.stderr)
        errors.setLevel(logging.ERROR)
        logger.addHandler(sink)
        logger.addHandler(errors)
        began = time.monotonic()
        current_phase = "admission"
        def persist():
            sink.flush()
            os.fsync(sink.stream.fileno())
        def observe(phase):
            global current_phase
            current_phase = phase
            logger.info("start operation=metric_export phase=%s elapsed_ms=%d", phase,
                        (time.monotonic() - began) * 1000)
            persist()
        try:
            export(args.reference, args.weights, args.output, args.side, metric=True,
                   native_aspect=args.metric_native_aspect, observe=observe)
            logger.info("done operation=metric_export elapsed_ms=%d", (time.monotonic() - began) * 1000)
            persist()
        except Exception:
            try:
                logger.exception("operation=metric_export failed phase=%s elapsed_ms=%d", current_phase,
                                 (time.monotonic() - began) * 1000)
                persist()
            except OSError as logging_error:
                # A secondary disk failure must not replace the export cause.
                sys.stderr.write("metric export log persistence failed: " + str(logging_error)[:512] + "\n")
            raise
        finally:
            sink.close()
