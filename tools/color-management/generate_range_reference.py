"""Independent OCIO CPU oracle for RGB code ranges around each image phase.

The existing pinned reference is read only. Alpha association, code-value
normalization and Float32 storage are explicit; no product shader is executed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import PyOpenColorIO as ocio


def f32(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


def processor(record, phase):
    names = ("OCIO_ACTIVE_DISPLAYS", "OCIO_ACTIVE_VIEWS", "OCIO_INACTIVE_COLORSPACES")
    saved = {key: os.environ.pop(key, None) for key in names}
    try:
        config = ocio.Config.CreateFromStream(record["config"])
    finally:
        for key, value in saved.items():
            if value is not None:
                os.environ[key] = value
    config.validate()
    if list(config.getEnvironmentVarNames()):
        raise ValueError("ambient context is not allowed")
    selection = record["selection"]

    def resolve(value):
        return record["manifest"]["roles"][value["name"]] if value["kind"] == "role" else value["name"]

    working = resolve(selection["working"])
    if phase == "input_to_working":
        native = config.getProcessor(resolve(selection["input"]), working)
    elif phase == "working_to_display":
        view = selection["display"]
        native = config.getProcessor(working, view["display"], view["view"], ocio.TRANSFORM_DIR_FORWARD)
    elif phase == "working_to_output":
        native = config.getProcessor(working, resolve(selection["output"]["space"]))
    else:
        raise ValueError("unknown explicit phase")
    return native.getDefaultCPUProcessor()


def generate(path):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != "bead7889440e39f8483904b088c948464223dbbc18f8cb0d8c5ec8b901b429a0":
        raise ValueError("pinned source reference changed")
    reference = json.loads(raw)
    if ocio.__version__ != "2.6.0":
        raise ValueError("requires OCIO 2.6.0")
    full, limited8, limited10 = [0., 1.], [16 / 255, 235 / 255], [64 / 1023, 940 / 1023]
    cases = []
    for stage, case in enumerate(reference["cases"]):
        source = case["processor"]
        cpu = processor(reference["configs"][case["config"]], source["phase"])
        for incoming, outgoing in [(limited8, full), (full, limited10), (limited8, limited10)]:
            low_in, high_in = map(f32, incoming)
            low_out, high_out = map(f32, outgoing)
            width_in, width_out = f32(high_in - low_in), f32(high_out - low_out)
            encoded, expected = [], []
            for raw_pixel in case["input"]:
                pixel = list(map(f32, raw_pixel))
                alpha = pixel[3]
                rgb = pixel[:3]
                if source["input_alpha"] == "premultiplied":
                    rgb = [f32(v / alpha) if alpha > 0 else 0. for v in rgb]
                coded = [f32(f32(v * width_in) + low_in) for v in rgb]
                if source["input_alpha"] == "premultiplied":
                    coded = [f32(v * alpha) for v in coded]
                encoded.append([*map(float, coded), float(alpha)])
                # Read back the encoded Float32 boundary, independently of the
                # original full-range expectation and the generated GPU shader.
                straight = coded
                if source["input_alpha"] == "premultiplied":
                    straight = [f32(v / alpha) if alpha > 0 else low_in for v in coded]
                decoded = [f32(f32(v - low_in) / width_in) for v in straight]
                result = [f32(v) for v in cpu.applyRGB(decoded)]
                result = [f32(f32(v * width_out) + low_out) for v in result]
                if source["output_alpha"] == "premultiplied":
                    result = [f32(v * alpha) for v in result]
                expected.append([*map(float, result), float(alpha)])
            cases.append(dict(stage=stage, input_range=incoming, output_range=outgoing,
                              input=encoded, expected=expected))
    return dict(runtime=ocio.__version__,
                reference_sha256=hashlib.sha256(raw).hexdigest(),
                absolute_tolerance=0.0001, relative_tolerance=0.0002, cases=cases)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    payload = generate(args.reference)
    with args.output.open("x", encoding="utf-8", newline="\n") as file:
        json.dump(payload, file, ensure_ascii=False, indent=2, allow_nan=False)
        file.write("\n")
