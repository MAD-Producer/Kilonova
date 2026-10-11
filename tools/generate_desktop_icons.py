"""Compile Motion Studio's Ardot SVG exports into portable path data.

This intentionally accepts only the geometry used by the owned icon library,
and rejects unsupported SVG features instead of silently changing their shape.
Run --check in CI after updating any source SVG or the icon manifest.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
ICONS = ROOT / "desktop/ui/icons"
NUMBER = r"[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:[eE][-+]?\d+)?"


def path_commands(text):
    tokens = re.findall(r"[A-Za-z]|" + NUMBER, text)
    if re.sub(r"[\s,]", "", text) != "".join(tokens):
        raise ValueError("unrecognized path token")
    commands, i, command = [], 0, None
    counts = {"M": 2, "L": 2, "Q": 4, "C": 6}
    while i < len(tokens):
        if tokens[i].isalpha():
            command = tokens[i]
            i += 1
        if command == "Z":
            commands.append(["Z"])
            command = None
            continue
        if command not in counts:
            raise ValueError(f"unsupported path command {command}")
        count = counts[command]
        values = [float(t) for t in tokens[i:i + count]]
        if len(values) != count or not all(math.isfinite(v) for v in values):
            raise ValueError("invalid path coordinates")
        commands.append([command, *values])
        i += count
        if command == "M":
            command = "L"
    return commands


def rounded_rect(x, y, w, h, rx=0, ry=0):
    rx, ry = min(rx, w / 2), min(ry, h / 2)
    k = 0.5522847498307936
    return [
        ["M", x + rx, y], ["L", x + w - rx, y],
        ["C", x + w - rx + k * rx, y, x + w, y + ry - k * ry, x + w, y + ry],
        ["L", x + w, y + h - ry],
        ["C", x + w, y + h - ry + k * ry, x + w - rx + k * rx, y + h, x + w - rx, y + h],
        ["L", x + rx, y + h],
        ["C", x + rx - k * rx, y + h, x, y + h - ry + k * ry, x, y + h - ry],
        ["L", x, y + ry],
        ["C", x, y + ry - k * ry, x + rx - k * rx, y, x + rx, y], ["Z"],
    ]


def compile_svg(source):
    # Git's Windows checkout may use CRLF; fingerprint canonical LF source.
    raw = source.read_text(encoding="utf-8").encode("utf-8")
    root = ET.fromstring(raw)
    if root.get("width") != "28" or root.get("height") != "28":
        raise ValueError(f"{source}: expected 28 by 28 SVG")
    shapes = []
    for element in root.iter():
        tag = element.tag.rsplit("}", 1)[-1]
        if tag in ("svg", "g"):
            if set(element.attrib) - {"width", "height", "viewBox", "clip-path"}:
                raise ValueError(f"unsupported group attributes: {element.attrib}")
            if element.get("clip-path") not in (None, "url(#clip_0)"):
                raise ValueError("unsupported clipping")
            continue
        if tag == "clipPath":
            # Only the canonical viewport clip used by the link icon is allowed.
            children = list(element)
            if element.get("id") != "clip_0" or len(children) != 1 or children[0].tag.rsplit("}", 1)[-1] != "rect" or children[0].attrib != {"width": "28", "height": "28"}:
                raise ValueError("unsupported clipping path")
            continue
        if any(element in list(parent) for parent in root.iter() if parent.tag.rsplit("}", 1)[-1] == "clipPath"):
            continue
        permitted = {"fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin", "transform", "d", "cx", "cy", "r", "x", "y", "width", "height", "rx", "ry"}
        if set(element.attrib) - permitted:
            raise ValueError(f"unsupported SVG attributes: {element.attrib}")
        fill, stroke = element.get("fill", "currentColor"), element.get("stroke", "none")
        if fill not in ("none", "currentColor") or stroke not in ("none", "currentColor"):
            raise ValueError("icons must use currentColor")
        # Translation and positive uniform scaling preserve the stroke contract.
        transform = element.get("transform", "matrix(1 0 0 1 0 0)")
        match = re.fullmatch(r"matrix\(([^)]+)\)", transform)
        values = [float(v) for v in match[1].split()] if match else []
        if len(values) != 6 or values[1:3] != [0, 0] or values[0] != values[3] or values[0] <= 0 or not all(math.isfinite(v) for v in values):
            raise ValueError("unsupported transform")
        if tag == "path":
            commands = path_commands(element.attrib["d"])
        elif tag == "rect":
            commands = rounded_rect(float(element.get("x", 0)), float(element.get("y", 0)), float(element.attrib["width"]), float(element.attrib["height"]), float(element.get("rx", 0)), float(element.get("ry", element.get("rx", 0))))
        elif tag == "circle":
            x, y, r = (float(element.attrib[k]) for k in ("cx", "cy", "r"))
            commands = rounded_rect(x - r, y - r, r * 2, r * 2, r, r)
        else:
            raise ValueError(f"unsupported SVG element {tag}")
        for command in commands:
            for i in range(1, len(command), 2):
                command[i] = round(command[i] * values[0] + values[4], 7)
                command[i + 1] = round(command[i + 1] * values[3] + values[5], 7)
        width = float(element.get("stroke-width", "1")) * values[0] if stroke != "none" else 0
        if width < 0 or not math.isfinite(width):
            raise ValueError("invalid stroke width")
        shapes.append({"commands": commands, "fill": fill != "none", "stroke": width,
                       "cap": element.get("stroke-linecap", "butt"), "join": element.get("stroke-linejoin", "miter")})
    return {"sha256": hashlib.sha256(raw).hexdigest(), "shapes": shapes}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    manifest = json.loads((ICONS / "manifest.json").read_text(encoding="utf-8"))
    entries = manifest["icons"]
    names = [e["name"] for e in entries]
    if len(set(names)) != len(names) or set(names) != {p.stem for p in ICONS.glob("*.svg")}:
        raise ValueError("manifest must enumerate each SVG exactly once")
    compiled = {e["name"]: compile_svg(ICONS / (e["name"] + ".svg")) for e in entries}
    generated = json.dumps(compiled, separators=(",", ":"), ensure_ascii=False) + "\n"
    destination = ICONS / "paths.json"
    if args.check:
        if not destination.exists() or destination.read_text(encoding="utf-8") != generated:
            raise ValueError("Outdated icon paths: run tools/generate_desktop_icons.py")
    else:
        destination.write_text(generated, encoding="utf-8", newline="\n")
    print(f"{'Verified' if args.check else 'Generated'} {len(entries)} desktop SVG icons")


if __name__ == "__main__":
    main()
