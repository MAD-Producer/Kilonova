"""Check explicit tool and Android test coverage without running device work."""
import argparse
import ast
import json
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = Path(__file__).with_name("validation_inventory.json")
ANDROID_SOURCE = "android/app/src/androidTest/java/com/motionstudio/editor"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_inventory(path=INVENTORY):
    inventory = json.loads(path.read_text(encoding="utf-8"))
    require(inventory.get("version") == 1, "Unsupported validation inventory version")
    return inventory


def check_tools(root, inventory):
    tools = {
        path.relative_to(root).as_posix(): path
        for path in (root / "tools").rglob("*.py")
        if "node_modules" not in path.parts and "__pycache__" not in path.parts
    }
    declared = inventory["tools"]
    require(set(tools) == set(declared),
            f"Tool inventory drift: unclassified={sorted(set(tools)-set(declared))}; missing={sorted(set(declared)-set(tools))}")
    workflow = (root / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    for name, entry in declared.items():
        mode = entry["mode"]
        require(mode in ("ci", "manual", "physical_device"), f"Unknown tool mode: {name}")
        require(bool(entry.get("reason")), f"Missing tool execution reason: {name}")
        tree = ast.parse(tools[name].read_text(encoding="utf-8"), filename=name)
        require(not any(isinstance(node, ast.Assert) for node in ast.walk(tree)),
                f"Tool uses an optimization-sensitive assert: {name}")
        if mode == "ci":
            runner = entry["runner"]
            if runner == ".github/workflows/ci.yml":
                if Path(name).name.startswith("test_ci_"):
                    require("unittest discover -s tools -p 'test_ci_*.py'" in workflow,
                            f"CI no longer discovers {name}")
                else:
                    require(name in workflow, f"CI no longer invokes {name}")
            else:
                runner_tree = ast.parse((root / runner).read_text(encoding="utf-8"))
                module = Path(name).stem
                require(any(isinstance(node, ast.ImportFrom) and node.module == module
                            or isinstance(node, ast.Import) and any(alias.name == module for alias in node.names)
                            for node in ast.walk(runner_tree)), f"{runner} no longer imports {name}")
        else:
            require(bool(entry.get("command")) and bool(entry.get("requires")),
                    f"Missing manual/device invocation or requirements: {name}")
    return len(tools)


def kotlin_test_classes(source):
    # Mask comments and literals so braces, class declarations and @Test text
    # inside shader/JSON fixtures cannot affect discovery. Preserve offsets.
    masked = re.sub(
        r'"""[\s\S]*?"""|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|//[^\n]*|/\*[\s\S]*?\*/',
        lambda match: " " * len(match[0]), source,
    )
    classes = {}
    for match in re.finditer(r"\bclass\s+(\w+)\b", masked):
        start = masked.find("{", match.end())
        require(start >= 0, f"Cannot resolve class body: {match[1]}")
        depth = 1
        end = start + 1
        while end < len(masked) and depth:
            depth += (masked[end] == "{") - (masked[end] == "}")
            end += 1
        require(depth == 0, f"Unclosed class body: {match[1]}")
        body = masked[start + 1:end - 1]
        methods = re.findall(r"@(?:org\.junit\.)?Test\b(?:(?!@(?:org\.junit\.)?Test\b|\bfun\b).)*\bfun\s+(\w+)", body, re.DOTALL)
        count = len(re.findall(r"@(?:org\.junit\.)?Test\b", body))
        require(count == len(methods), f"Cannot resolve @Test methods: {match[1]}")
        if count:
            require(match[1] not in classes, f"Duplicate Android class: {match[1]}")
            require(len(methods) == len(set(methods)), f"Duplicate Android test method: {match[1]}")
            classes[match[1]] = methods
    return classes


def android_suite(source, inventory):
    discovered = {}
    for path in sorted(source.glob("*.kt")):
        for name, methods in kotlin_test_classes(path.read_text(encoding="utf-8")).items():
            require(name not in discovered, f"Duplicate Android test class: {name}")
            discovered[name] = (path.name, methods)
    declared = inventory["android_tests"]
    require(set(discovered) == set(declared),
            f"Android inventory drift: unclassified={sorted(set(discovered)-set(declared))}; missing={sorted(set(declared)-set(discovered))}")
    selectors = []
    expected = 0
    for name, entry in declared.items():
        file, methods = discovered[name]
        require(file == entry["source"], f"Android test moved without updating inventory: {name}")
        mode = entry["mode"]
        require(mode in ("ci", "manual", "physical_device"), f"Unknown Android test mode: {name}")
        require(bool(entry.get("reason")), f"Missing Android execution reason: {name}")
        if mode == "ci":
            selected = entry["methods"]
            if selected == "*":
                selectors.append(name)
                expected += len(methods)
            else:
                require(isinstance(selected, list) and selected and len(selected) == len(set(selected)),
                        f"Invalid Android method selection: {name}")
                require(set(selected) <= set(methods), f"Selected Android test method is missing: {name}")
                require(bool(entry.get("remaining_methods_reason")), f"Unclassified remaining methods: {name}")
                selectors.extend(name + "#" + method for method in selected)
                expected += len(selected)
        else:
            require(bool(entry.get("command")), f"Missing manual/device Android invocation: {name}")
    require(selectors and expected > 0, "Android CI suite must not be empty")
    return selectors, expected


def check_effect_libraries(root, inventory, validator):
    """Use the Rust loader's schema/WGSL checks, rather than a Python replica."""
    require(validator.is_file(), f"Effect validator not found: {validator}")
    def run(*arguments):
        result = subprocess.run([str(validator), *map(str, arguments)], capture_output=True, text=True, encoding="utf-8")
        require(result.returncode == 0, f"Effect library validation failed: {' '.join(map(str, arguments))}\n{result.stdout}{result.stderr}")
    with tempfile.TemporaryDirectory(prefix="motion-effect-inventory-") as directory:
        for index, library in enumerate(inventory["effect_libraries"]):
            source = root / library
            run("plan", source / "manifest.json")
            package = Path(directory) / f"library-{index}.msfx"
            run("pack", source, package)
            run("check", package)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--inventory", type=Path, default=INVENTORY)
    parser.add_argument("--effect-validator", type=Path)
    args = parser.parse_args()
    inventory = read_inventory(args.inventory)
    tools = check_tools(args.root, inventory)
    selectors, expected = android_suite(args.root / ANDROID_SOURCE, inventory)
    if args.effect_validator:
        check_effect_libraries(args.root, inventory, args.effect_validator.resolve())
    print(json.dumps({"tools": tools, "androidSelectors": len(selectors), "androidTests": expected,
                      "effectLibrariesChecked": bool(args.effect_validator)}, indent=2))


if __name__ == "__main__":
    main()
