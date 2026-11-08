"""Prepare an explicit OCIO 2.6 GPU program outside the editor process.

The selected catalogue must already be a closed export_config.py bundle. No
default configuration, role, view, context or dynamically changing uniform is
substituted. Native exceptions and IO errors propagate with their original cause.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path

import PyOpenColorIO as ocio


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON object field")
        result[key] = value
    return result


def read_json(path, limit):
    with path.open("rb") as source:
        data = source.read(limit + 1)
    if len(data) > limit:
        raise ValueError("JSON input exceeds preparation capacity")
    return json.loads(data, object_pairs_hook=unique_object)


def ordered(value, fields):
    if set(value) != set(fields):
        raise ValueError("unexpected or missing preparation fields")
    return {name: value[name] for name in fields}


def version(value):
    return ordered(value, ("major", "minor", "patch"))


def canonical(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode("utf-8")


def bounded_name(value):
    if not isinstance(value, str) or not 1 <= len(value.encode("utf-8")) <= 256 or any(ord(c) < 32 or ord(c) == 127 for c in value):
        raise ValueError("invalid or oversized explicit selection name")
    return value


def selection_identity(manifest, selection):
    manifest = ordered(manifest, ("version", "ocio_version", "origin", "config", "resources", "context", "spaces", "roles", "displays"))
    if manifest["version"] != 1 or manifest["ocio_version"] != {"major": 2, "minor": 6, "patch": 0}:
        raise ValueError("requires the current explicit OCIO 2.6.0 catalogue")
    manifest["ocio_version"] = version(manifest["ocio_version"])
    origin = manifest["origin"]
    if origin["kind"] == "builtin":
        origin = ordered(origin, ("kind", "id", "aces"))
        origin["aces"] = version(origin["aces"])
    elif origin["kind"] == "authored":
        origin = ordered(origin, ("kind", "id"))
    else:
        raise ValueError("unsupported explicit configuration origin")
    manifest["origin"] = origin
    manifest["config"] = ordered(manifest["config"], ("bytes", "sha256"))
    if manifest["resources"] or manifest["context"]:
        raise ValueError("processor preparation requires the closed resource-free catalogue")
    manifest["spaces"] = {key: ordered(value, ("encoding", "reference", "is_data")) for key, value in sorted(manifest["spaces"].items())}
    manifest["roles"] = dict(sorted(manifest["roles"].items()))
    manifest["displays"] = dict(sorted(manifest["displays"].items()))
    config_key = list(hashlib.sha256(b"motion-color-config-v1\0" + canonical(manifest)).digest())
    selection = ordered(selection, ("config_key", "aces", "input", "working", "display", "output"))
    if selection["config_key"] != config_key or selection["aces"] != origin.get("aces"):
        raise ValueError("selection differs from explicit config or ACES identity")
    if selection["aces"] is not None:
        selection["aces"] = version(selection["aces"])

    def space(reference):
        reference = ordered(reference, ("kind", "name"))
        bounded_name(reference["name"])
        if reference["kind"] == "role":
            target = manifest["roles"][reference["name"]]
        elif reference["kind"] == "space":
            target = reference["name"]
        else:
            raise ValueError("unsupported color-space reference")
        if manifest["spaces"][target]["is_data"]:
            raise ValueError("data spaces must bypass color processors")
        return reference, target

    def display(value):
        value = ordered(value, ("display", "view"))
        bounded_name(value["display"])
        bounded_name(value["view"])
        if value["view"] not in manifest["displays"][value["display"]]:
            raise ValueError("explicit view is absent")
        return value

    selection["input"], source = space(selection["input"])
    selection["working"], working = space(selection["working"])
    selection["display"] = display(selection["display"])
    output = selection["output"]
    if output["kind"] == "space":
        output = ordered(output, ("kind", "space"))
        output["space"], target = space(output["space"])
    elif output["kind"] == "display":
        output = ordered(output, ("kind", "display_view"))
        output["display_view"] = display(output["display_view"])
        target = output["display_view"]
    else:
        raise ValueError("unsupported output selection")
    selection["output"] = output
    key = list(hashlib.sha256(b"motion-color-selection-v1\0" + canonical(selection)).digest())
    return manifest, selection, key, source, working, target


def prepare(bundle, selection, phase, input_alpha, output_alpha):
    import numpy
    if ocio.__version__ != "2.6.0" or numpy.__version__ != "2.3.5":
        raise ValueError("requires pinned OpenColorIO 2.6.0 and NumPy 2.3.5")
    manifest = read_json(bundle / "manifest.json", 1024 * 1024)
    manifest, selection, key, source, working, target = selection_identity(manifest, selection)
    with (bundle / "config.ocio").open("rb") as file:
        data = file.read(8 * 1024 * 1024 + 1)
    if len(data) > 8 * 1024 * 1024 or manifest["config"] != {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}:
        raise ValueError("configuration bytes differ from the selected content identity")
    overrides = ("OCIO_ACTIVE_DISPLAYS", "OCIO_ACTIVE_VIEWS", "OCIO_INACTIVE_COLORSPACES")
    saved = {name: os.environ.get(name) for name in overrides}
    try:
        for name in overrides:
            os.environ.pop(name, None)
        config = ocio.Config.CreateFromStream(data.decode("utf-8"))
    finally:
        for name, value in saved.items():
            if value is not None:
                os.environ[name] = value
    config.validate()
    if list(config.getEnvironmentVarNames()) or list(config.getCurrentContext().getStringVars()):
        raise ValueError("ambient OCIO context is not a closed processor")
    if phase == "input_to_working":
        processor = config.getProcessor(source, working)
    elif phase == "working_to_display":
        view = selection["display"]
        processor = config.getProcessor(working, view["display"], view["view"], ocio.TRANSFORM_DIR_FORWARD)
    elif phase == "working_to_output":
        processor = config.getProcessor(working, target) if isinstance(target, str) else config.getProcessor(working, target["display"], target["view"], ocio.TRANSFORM_DIR_FORWARD)
    else:
        raise ValueError("unsupported explicit processor phase")
    if list(processor.getProcessorMetadata().getFiles()):
        raise ValueError("processor references external files")
    desc = ocio.GpuShaderDesc.CreateShaderDesc()
    desc.setLanguage(ocio.GPU_LANGUAGE_GLSL_VK_4_6)
    desc.setAllowTexture1D(False)
    desc.setTextureMaxWidth(4096)
    desc.setFunctionName("motion_ocio_transform")
    desc.setResourcePrefix("motion_ocio_")
    processor.getDefaultGPUProcessor().extractGpuShaderInfo(desc)
    if list(desc.get3DTextures()) or list(desc.getUniforms()):
        raise ValueError("3D textures and dynamic uniforms need explicit additional admission")
    textures = list(desc.getTextures())
    if len(textures) > 4:
        raise ValueError("processor table count exceeds capacity")
    tables = []
    for texture in textures:
        if texture.dimensions != ocio.GpuShaderCreator.TEXTURE_2D or texture.width * texture.height > 262144:
            raise ValueError("processor table dimensions exceed capacity")
        if texture.interpolation not in (ocio.INTERP_NEAREST, ocio.INTERP_LINEAR):
            raise ValueError("unsupported explicit table interpolation")
        values = [float(v) for v in texture.getValues().reshape(-1)]
        if not all(math.isfinite(v) for v in values):
            raise ValueError("nonfinite processor table")
        tables.append({"texture_name": texture.textureName, "sampler_name": texture.samplerName,
                       "binding": texture.textureShaderBindingIndex, "width": texture.width, "height": texture.height,
                       "channels": 1 if texture.channel == ocio.GpuShaderCreator.TEXTURE_RED_CHANNEL else 3,
                       "interpolation": "nearest" if texture.interpolation == ocio.INTERP_NEAREST else "linear", "values": values})
    shader = desc.getShaderText()
    if len(shader.encode("utf-8")) > 256 * 1024:
        raise ValueError("processor shader exceeds capacity")
    return {"version": 1, "ocio_version": version(manifest["ocio_version"]), "config_key": selection["config_key"],
            "selection_key": key, "phase": phase, "input_alpha": input_alpha, "output_alpha": output_alpha,
            "shader": shader, "tables": tables}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config-directory", type=Path, required=True)
    parser.add_argument("--selection", type=Path, required=True)
    parser.add_argument("--phase", choices=("input_to_working", "working_to_display", "working_to_output"), required=True)
    parser.add_argument("--input-alpha", choices=("straight", "premultiplied"), required=True)
    parser.add_argument("--output-alpha", choices=("straight", "premultiplied"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = prepare(args.config_directory, read_json(args.selection, 65536), args.phase, args.input_alpha, args.output_alpha)
    payload = canonical(result) + b"\n"
    if len(payload) > 16 * 1024 * 1024:
        raise ValueError("processor export exceeds capacity")
    # Exclusive creation: a failed export never replaces an existing processor.
    with args.output.open("xb") as destination:
        destination.write(payload)
    print(json.dumps({"phase": args.phase, "shader_bytes": len(result["shader"].encode("utf-8")), "tables": len(result["tables"])}))


if __name__ == "__main__":
    main()
