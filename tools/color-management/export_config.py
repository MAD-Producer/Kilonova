"""Export a closed catalogue of explicitly versioned OCIO built-in configs.

This is an external preparation tool, not an installed application dependency.
No environment-selected config, default role or display/view fallback is used.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path

import PyOpenColorIO as ocio

BUILTINS = {
    "cg-config-v2.2.0_aces-v1.3_ocio-v2.4": (1, 3, 0),
    "cg-config-v4.0.0_aces-v2.0_ocio-v2.5": (2, 0, 0),
    "cg-config-v5.0.0_aces-v2.1_ocio-v2.6": (2, 1, 0),
}


def version(values):
    return dict(zip(("major", "minor", "patch"), values))


def identity(data):
    return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def snapshot(config, origin):
    """Snapshot an already constructed config; reject all ambient dependencies.

    Every space's to/from-reference processor and every display/view is traversed
    by native OCIO. File dependencies are rejected, not omitted from the identity.
    General file configs require a separate explicit dependency-closure importer.
    """
    if ocio.__version__ != "2.6.0":
        raise ValueError("requires the pinned OpenColorIO 2.6.0 reference runtime")
    if list(config.getEnvironmentVarNames()) or list(config.getCurrentContext().getStringVars()):
        raise ValueError("ambient OCIO context is not a closed configuration")
    # OCIO captures global display/view overrides at Config construction. Reparse
    # its explicitly serialized content with those overrides excluded; changing
    # the environment after construction alone does not remove the captured
    # values. This scoped operation belongs only in this single-threaded external
    # preparation tool. It must never run inside the editor/host process.
    overrides = ("OCIO_ACTIVE_DISPLAYS", "OCIO_ACTIVE_VIEWS", "OCIO_INACTIVE_COLORSPACES")
    saved = {key: os.environ.get(key) for key in overrides}
    try:
        for key in overrides:
            os.environ.pop(key, None)
        config = ocio.Config.CreateFromStream(config.serialize())
    finally:
        for key, value in saved.items():
            if value is not None:
                os.environ[key] = value
    config.validate()
    transforms = []
    for space in config.getColorSpaces(ocio.SEARCH_REFERENCE_SPACE_ALL, ocio.COLORSPACE_ALL):
        transforms.extend(space.getTransform(direction) for direction in (ocio.COLORSPACE_DIR_TO_REFERENCE, ocio.COLORSPACE_DIR_FROM_REFERENCE))
    for view in config.getViewTransforms():
        transforms.extend(view.getTransform(direction) for direction in (ocio.VIEWTRANSFORM_DIR_TO_REFERENCE, ocio.VIEWTRANSFORM_DIR_FROM_REFERENCE))
    for named in config.getNamedTransforms(ocio.NAMEDTRANSFORM_ALL):
        transforms.extend(named.getTransform(direction) for direction in (ocio.TRANSFORM_DIR_FORWARD, ocio.TRANSFORM_DIR_INVERSE))
    for look in config.getLooks():
        transforms.extend((look.getTransform(), look.getInverseTransform()))
    visited = 0
    while transforms:
        transform = transforms.pop()
        visited += 1
        if visited > 100000:
            raise ValueError("configuration transform count exceeds preparation limit")
        if isinstance(transform, ocio.FileTransform):
            raise ValueError("external file dependencies need an explicit resource closure")
        if isinstance(transform, ocio.GroupTransform):
            transforms.extend(transform)
    reference = config.getRoleColorSpace("scene_linear")
    if not reference:
        raise ValueError("explicit scene_linear role is missing")

    def check_processor(processor):
        if list(processor.getProcessorMetadata().getFiles()):
            raise ValueError("external file dependencies need an explicit resource closure")

    spaces = {}
    for space in config.getColorSpaces(ocio.SEARCH_REFERENCE_SPACE_ALL, ocio.COLORSPACE_ALL):
        label = space.getName()
        for source, target in ((label, reference), (reference, label)):
            check_processor(config.getProcessor(source, target))
        spaces[label] = {
            "encoding": space.getEncoding(),
            "reference": "scene" if space.getReferenceSpaceType() == ocio.REFERENCE_SPACE_SCENE else "display",
            "is_data": space.isData(),
        }
    displays = {}
    for display in config.getDisplaysAll():
        displays[display] = sorted(set(config.getViews(ocio.VIEW_DISPLAY_DEFINED, display)) | set(config.getViews(ocio.VIEW_SHARED, display)))
        for view in displays[display]:
            check_processor(config.getProcessor(reference, display, view, ocio.TRANSFORM_DIR_FORWARD))
    data = config.serialize().encode("utf-8")
    manifest = {
        "version": 1,
        "ocio_version": version((2, 6, 0)),
        "origin": origin,
        "config": identity(data),
        "resources": {},
        "context": {},
        "spaces": dict(sorted(spaces.items())),
        "roles": dict(sorted(config.getRoles())),
        "displays": dict(sorted(displays.items())),
    }
    return manifest, data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--builtin", choices=BUILTINS, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    # Native preparation finishes before any output is written. IO failures also
    # propagate with their original traceback; consumers verify the whole bundle.
    manifest, data = snapshot(
        ocio.Config.CreateFromBuiltinConfig(args.builtin),
        {"kind": "builtin", "id": args.builtin, "aces": version(BUILTINS[args.builtin])},
    )
    payload = (json.dumps(manifest, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")
    if len(payload) > 1024 * 1024 or len(data) > 8 * 1024 * 1024:
        raise ValueError("export exceeds the native catalogue limits")
    args.output.mkdir(parents=True, exist_ok=False)
    (args.output / "config.ocio").write_bytes(data)
    (args.output / "manifest.json").write_bytes(payload)
    print(json.dumps({"builtin": args.builtin, "config": manifest["config"], "spaces": len(manifest["spaces"]), "roles": len(manifest["roles"]), "displays": len(manifest["displays"])}, sort_keys=True))


if __name__ == "__main__":
    main()
