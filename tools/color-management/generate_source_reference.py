"""Offline original-file color oracle; requires the separately pinned OCIO 2.6.0.

No product runtime or default configuration is installed. Config bytes and
programs stay in the already attributed reference; only new CPU outputs and
independently authored integer sample grids are written here.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import PyOpenColorIO as ocio


def apply(cpu, values, incoming, outgoing):
    result = []
    for pixel in values:
        value = pixel.copy()
        if incoming == "premultiplied":
            value[:3] = [v / pixel[3] if pixel[3] > 0 else 0. for v in value[:3]]
        value[:3] = cpu.applyRGB(value[:3])
        if outgoing == "premultiplied":
            value[:3] = [v * pixel[3] for v in value[:3]]
        result.append(list(struct.unpack("<4f", struct.pack("<4f", *value))))
    return result


def generate(reference):
    raw = reference.read_bytes()
    ref = json.loads(raw)
    if ocio.__version__ != "2.6.0" or ref["runtime"] != "2.6.0":
        raise ValueError("requires pinned original OCIO 2.6.0 reference")
    samples = [[32767, 32768, 32769, 65535], [32768, 32769, 32770, 65535],
               [16384, 8192, 4096, 32768], [60000, 40000, 20000, 65535],
               [65535, 65535, 65535, 65535], [0, 0, 0, 65535],
               [65535, 1234, 4321, 0], [1, 2, 3, 1], [12, 345, 6789, 40000],
               [6000, 2000, 9000, 12345], [10000, 16000, 20000, 40000],
               [50000, 40000, 30000, 65535]]
    # Mirror only the format's normalized Float32 storage, not the GPU transform.
    f32 = lambda v: struct.unpack("<f", struct.pack("<f", v))[0]
    pixels = [[f32(v / 65535.) for v in p] for p in samples]
    cases = []
    for index, record in enumerate(ref["configs"]):
        encoded = record["config"].encode("utf-8")
        if record["manifest"]["config"] != dict(bytes=len(encoded), sha256=hashlib.sha256(encoded).hexdigest()):
            raise ValueError("original configuration bytes changed")
        names = ("OCIO_ACTIVE_DISPLAYS", "OCIO_ACTIVE_VIEWS", "OCIO_INACTIVE_COLORSPACES")
        saved = {k: os.environ.pop(k, None) for k in names}
        try:
            config = ocio.Config.CreateFromStream(record["config"])
        finally:
            for k, value in saved.items():
                if value is not None:
                    os.environ[k] = value
        config.validate()
        if list(config.getEnvironmentVarNames()):
            raise ValueError("ambient context is not allowed")
        selection = record["selection"]

        def resolve(value):
            return record["manifest"]["roles"][value["name"]] if value["kind"] == "role" else value["name"]

        working = resolve(selection["working"])
        cpu_input = config.getProcessor(resolve(selection["input"]), working).getDefaultCPUProcessor()
        for first, upstream in enumerate(ref["cases"]):
            p = upstream["processor"]
            if upstream["config"] != index or p["phase"] != "input_to_working":
                continue
            intermediate = apply(cpu_input, pixels, p["input_alpha"], p["output_alpha"])
            for second, downstream in enumerate(ref["cases"]):
                q = downstream["processor"]
                if downstream["config"] != index or q["phase"] == "input_to_working" or p["output_alpha"] != q["input_alpha"]:
                    continue
                if q["phase"] == "working_to_display":
                    view = selection["display"]
                    cpu = config.getProcessor(working, view["display"], view["view"], ocio.TRANSFORM_DIR_FORWARD).getDefaultCPUProcessor()
                else:
                    cpu = config.getProcessor(working, resolve(selection["output"]["space"])).getDefaultCPUProcessor()
                cases.append(dict(config=index, stages=[first, second], intermediate=intermediate,
                                  expected=apply(cpu, intermediate, q["input_alpha"], q["output_alpha"])))
    return dict(runtime=ocio.__version__, reference_sha256=hashlib.sha256(raw).hexdigest(),
                absolute_tolerance=0.0001, relative_tolerance=0.0002,
                width=3, height=4, samples=samples, cases=cases)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    with args.output.open("x", encoding="utf-8", newline="\n") as file:
        json.dump(generate(args.reference), file, ensure_ascii=False, indent=2, allow_nan=False)
        file.write("\n")
