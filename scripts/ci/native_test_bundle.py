#!/usr/bin/env python3
"""Retain authentic native Rust test inputs for replay on smaller machines.

As RUSTC_WRAPPER this runs selected real compiler commands unchanged and
forwards their exact exit. `pack` archives their complete production library,
matching dependency closure, generated inputs and native test executables.
It never supplies compiler outputs or changes a test verdict.
"""

import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import stat
import subprocess
import sys
import tarfile
import time

ROOT = Path(__file__).resolve().parents[2]
RECORDS = ROOT / "target/native-test-records"
MAX_INPUT_BYTES = 3 * 1024**3
MAX_ARCHIVE_BYTES = 1024**3
SAFE_ENV = {
    "PATH", "LD_LIBRARY_PATH", "OUT_DIR", "RUSTUP_TOOLCHAIN", "SOURCE_DATE_EPOCH",
    "CARGO_MANIFEST_DIR", "CARGO_MANIFEST_PATH", "CARGO_CRATE_NAME",
    "CARGO_PRIMARY_PACKAGE", "CARGO_PKG_VERSION", "CARGO_PKG_VERSION_MAJOR",
    "CARGO_PKG_VERSION_MINOR", "CARGO_PKG_VERSION_PATCH", "CARGO_PKG_VERSION_PRE",
    "CARGO_PKG_NAME", "CARGO_PKG_AUTHORS", "CARGO_PKG_DESCRIPTION", "CARGO_PKG_HOMEPAGE",
    "CARGO_PKG_REPOSITORY", "CARGO_PKG_LICENSE", "CARGO_PKG_LICENSE_FILE", "CARGO_PKG_RUST_VERSION",
}


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def source_identity():
    # Generated/ignored build files are expected; tracked source must be exact.
    subprocess.run(["git", "diff", "--quiet"], cwd=ROOT, check=True)
    subprocess.run(["git", "diff", "--cached", "--quiet"], cwd=ROOT, check=True)
    return {"checkout": git("rev-parse", "HEAD"), "tree": git("rev-parse", "HEAD^{tree}")}


def value(args, flag):
    return args[args.index(flag) + 1]


def products(args):
    directory = Path(value(args, "--out-dir"))
    crate = value(args, "--crate-name")
    extra = next(args[i + 1].split("=", 1)[1] for i, item in enumerate(args) if item == "-C" and args[i + 1].startswith("extra-filename="))
    if "--test" in args:
        return [directory / (crate + extra)]
    return [directory / ("lib" + crate + extra + suffix) for suffix in (".rlib", ".rmeta")]


def file_state(path, hashed=False):
    if not path.exists():
        return None
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode):
        raise RuntimeError("native output is not a regular file: " + str(path))
    result = {"bytes": info.st_size, "mtime_ns": info.st_mtime_ns, "ctime_ns": info.st_ctime_ns, "inode": info.st_ino}
    if hashed:
        result["sha256"] = digest(path)
    return result


def wrapper(compiler, args):
    if "--crate-name" in args and "--out-dir" in args and "--target" not in args and os.environ.get("CARGO_MANIFEST_DIR") == str(ROOT / "crates/core"):
        crate = value(args, "--crate-name")
        if crate in {"labwired_core", "avr", "gates"}:
            record = {
                "compiler": compiler, "argv": args, "cwd": os.getcwd(),
                "env": {key: os.environ[key] for key in sorted(SAFE_ENV) if key in os.environ},
                "source": source_identity(),
            }
            RECORDS.mkdir(parents=True, exist_ok=True)
            name = hashlib.sha256(json.dumps(record, sort_keys=True).encode()).hexdigest()
            path = RECORDS / (name + ".json")
            record["before"] = {str(product): file_state(product) for product in products(args)}
            record.update(started_ns=time.time_ns(), compiler_exit=None, complete=False)
            path.write_text(json.dumps(record, indent=2) + "\n")
            result = subprocess.run([compiler, *args], close_fds=False)
            record.update(compiler_exit=result.returncode, completed_ns=time.time_ns())
            try:
                record["source_after"] = source_identity()
                record["outputs"] = {str(product): file_state(product, hashed=True) for product in products(args)}
                record["complete"] = result.returncode == 0 and record["source_after"] == record["source"] and all(
                    output is not None and output["mtime_ns"] >= record["started_ns"] and
                    {key: data for key, data in output.items() if key != "sha256"} != record["before"][product]
                    for product, output in record["outputs"].items()
                )
            except Exception as error:
                record["capture_error"] = str(error)
            try:
                path.write_text(json.dumps(record, indent=2) + "\n")
            except OSError as error:
                print("Native compiler recording failed: " + str(error), file=sys.stderr)
            # Preserve the real compiler's exit or signal, including failure.
            if result.returncode < 0:
                compiler_signal = -result.returncode
                if compiler_signal not in {signal.SIGKILL, signal.SIGSTOP}:
                    signal.signal(compiler_signal, signal.SIG_DFL)
                os.kill(os.getpid(), compiler_signal)
            return result.returncode
    os.execv(compiler, [compiler, *args])


def target_path(path, directory=False):
    target = ROOT / "target"
    if target.is_symlink() or target.resolve() != target:
        raise RuntimeError("noncanonical Cargo target root")
    path = Path(path)
    resolved = path.resolve(strict=True)
    if resolved == target or not resolved.is_relative_to(target) or path.is_symlink():
        raise RuntimeError("native input escapes the Cargo target: " + str(path))
    for parent in path.parents:
        if parent == target:
            break
        if parent.is_symlink():
            raise RuntimeError("symlink in native input path: " + str(path))
    if directory and not resolved.is_dir():
        raise RuntimeError("native generated root is not a directory")
    return resolved


def validate_completion(record):
    required = {"compiler", "argv", "cwd", "env", "source", "source_after", "before", "outputs", "started_ns", "completed_ns", "compiler_exit", "complete"}
    if not required.issubset(record) or set(record["env"]) - SAFE_ENV:
        raise RuntimeError("incomplete compiler record or unsafe environment fields")
    if record.get("compiler_exit") != 0 or record.get("complete") is not True or record.get("source_after") != record["source"]:
        raise RuntimeError("incomplete or unsuccessful real compiler invocation")
    if not 0 < record.get("started_ns", 0) <= record.get("completed_ns", 0):
        raise RuntimeError("invalid compiler completion interval")
    expected_products = {str(path) for path in products(record["argv"])}
    if set(record["before"]) != expected_products or set(record["outputs"]) != expected_products:
        raise RuntimeError("incomplete compiler output set")
    for product in products(record["argv"]):
        target_path(product)
        observed = file_state(product, hashed=True)
        recorded = record.get("outputs", {}).get(str(product))
        if observed != recorded or observed is None or not record["started_ns"] <= observed["mtime_ns"] <= record["completed_ns"]:
            raise RuntimeError("stale, missing or altered native compiler output: " + str(product))
        if {key: data for key, data in observed.items() if key != "sha256"} == record.get("before", {}).get(str(product)):
            raise RuntimeError("compiler did not freshly produce native output")


def dependency_crates(host):
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--format-version=1", "--filter-platform", host,
    ], cwd=ROOT))
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    wanted = {next(key for key, package in packages.items() if package["name"] == "labwired-core")}
    pending = list(wanted)
    while pending:
        for dependency in nodes[pending.pop()]["deps"]:
            if dependency["pkg"] not in wanted:
                wanted.add(dependency["pkg"])
                pending.append(dependency["pkg"])
    return {
        target["name"].replace("-", "_")
        for package_id in wanted for target in packages[package_id]["targets"]
        if set(target["kind"]) & {"lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"}
    }


def select_libraries(selected, libraries):
    # Cargo can compile multiple full Core libraries in one workspace command.
    # Authenticate the primary by the actual AVR extern, retaining every other
    # variant referenced by a selected harness or library dependency.
    def core_externs(record):
        args = record["argv"]
        return [target_path(args[i + 1].split("=", 1)[1])
                for i, item in enumerate(args) if item == "--extern" and args[i + 1].startswith("labwired_core=")]

    def matching(path):
        candidates = [record for record in libraries if path in products(record["argv"])]
        if len(candidates) != 1:
            raise RuntimeError("missing or ambiguous exact Core library for " + str(path))
        return candidates[0]

    if "avr" in selected:
        paths = core_externs(selected["avr"])
        if len(paths) != 1:
            raise RuntimeError("AVR must reference exactly one genuine Core library")
        primary = matching(paths[0])
    elif len(libraries) == 1:
        primary = libraries[0]
    elif libraries:
        raise RuntimeError("ambiguous Core library without an AVR compiler record")
    else:
        return []
    selected["library"] = primary
    retained = [primary]
    pending = list(selected.values())
    while pending:
        for path in core_externs(pending.pop()):
            record = matching(path)
            if record not in retained:
                retained.append(record)
                pending.append(record)
    return retained[1:]


def pack(mode, output, available=False):
    expected = {"default"} | ({"event-scheduler", "jit-framework"} if mode == "workspace" else set())
    source = source_identity()
    selected = {}
    libraries = []
    for path in sorted(RECORDS.glob("*.json")):
        record = json.loads(path.read_text())
        args = record["argv"]
        features = {args[i + 1][9:-1] for i, item in enumerate(args) if item == "--cfg" and args[i + 1].startswith('feature="')}
        if record["source"] != source or features != expected:
            continue
        validate_completion(record)
        crate = value(args, "--crate-name")
        role = "core-unit" if crate == "labwired_core" and "--test" in args else ("library" if crate == "labwired_core" else crate)
        if role == "library":
            libraries.append(record)
            continue
        if role in selected:
            raise RuntimeError("ambiguous compiler record for " + role)
        selected[role] = record
    variants = select_libraries(selected, libraries)
    required = {"library", "core-unit", "avr", "gates"}
    if not selected or (set(selected) != required and not available):
        raise RuntimeError("missing genuine native compiler records: " + str(sorted(selected)))
    first = selected.get("library", next(iter(selected.values())))
    selector = first["compiler"]
    records = [(role, record) for role, record in selected.items()] + [("library", record) for record in variants]
    if any(record["compiler"] != selector for _, record in records):
        raise RuntimeError("compiler identity changed within the native bundle")
    version = subprocess.check_output([selector, "-Vv"], text=True)
    if not version.startswith("rustc 1.95.0 "):
        raise RuntimeError("unexpected compiler: " + version.splitlines()[0])
    host = next(line[6:] for line in version.splitlines() if line.startswith("host: "))
    sysroot = Path(subprocess.check_output([selector, "--print", "sysroot"], text=True).strip())
    compiler = sysroot / "bin/rustc"
    drivers = list(sysroot.joinpath("lib").glob("librustc_driver*.so"))
    if not drivers or subprocess.check_output([str(compiler), "-Vv"], text=True) != version:
        raise RuntimeError("unverified native compiler implementation")
    deps = target_path(value(first["argv"], "--out-dir"), directory=True)
    crates = dependency_crates(host)
    files = {"provenance/" + name: ROOT / name for name in ("Cargo.lock", "LICENSE")}
    core_products = {product for role, record in records if role == "library" for product in products(record["argv"])}
    for path in deps.iterdir():
        if path.name.startswith("liblabwired_core-") and path not in core_products:
            continue
        if path.is_file() and path.suffix in {".rlib", ".rmeta", ".so"} and any(path.name.startswith("lib" + name + "-") for name in crates):
            files["deps/" + path.name] = target_path(path)
    path_map = {str(deps): "deps"}
    for role, record in records:
        args = record["argv"]
        extra = next(args[i + 1].split("=", 1)[1] for i, item in enumerate(args) if item == "-C" and args[i + 1].startswith("extra-filename="))
        if role == "library":
            product = deps / ("liblabwired_core" + extra + ".rlib")
            with product.open("rb") as stream:
                if stream.read(8) != b"!<arch>\n":
                    raise RuntimeError("full production rlib is absent")
            if "--emit=dep-info,metadata,link" not in args or "--test" in args:
                raise RuntimeError("production library was not compiled in full")
        else:
            product = Path(value(args, "--out-dir")) / (value(args, "--crate-name") + extra)
            with product.open("rb") as stream:
                if stream.read(4) != b"\x7fELF":
                    raise RuntimeError("native test executable is absent: " + role)
            files["bin/" + role] = product
        for i, item in enumerate(args):
            if item == "--extern":
                path = Path(args[i + 1].split("=", 1)[1])
                if path != deps / path.name or "deps/" + path.name not in files:
                    raise RuntimeError("missing exact compiler dependency: " + str(path))
        generated = [record["env"]["OUT_DIR"]] if "OUT_DIR" in record["env"] else []
        generated += [args[i + 1][7:] for i, item in enumerate(args) if item == "-L" and args[i + 1].startswith("native=")]
        for original in generated:
            root = target_path(original, directory=True)
            if str(root) not in path_map:
                prefix = "generated/" + hashlib.sha256(str(root).encode()).hexdigest()[:16]
                path_map[str(root)] = prefix
                count = 0
                for path in root.rglob("*"):
                    count += 1
                    if count > 20000:
                        raise RuntimeError("generated native input exceeds the file bound")
                    if path.is_symlink():
                        raise RuntimeError("symlink in generated native input: " + str(path))
                    if path.is_file():
                        files[prefix + "/" + str(path.relative_to(root))] = target_path(path)
                        if sum(item.stat().st_size for item in files.values()) > MAX_INPUT_BYTES:
                            raise RuntimeError("native bundle exceeds the 3 GiB input bound")
    total = sum(path.stat().st_size for path in files.values())
    if any(not stat.S_ISREG(path.lstat().st_mode) for path in files.values()):
        raise RuntimeError("nonregular native bundle payload")
    if total > MAX_INPUT_BYTES:
        raise RuntimeError("native bundle exceeds the 3 GiB input bound")
    manifest = {
        "schema": 1, "mode": mode, "source": source, "architecture": platform.machine(),
        "compiler": {"version": version, "selector": selector, "sysroot": str(sysroot), "sha256": digest(compiler), "drivers": {
            path.name: {"bytes": path.stat().st_size, "sha256": digest(path)}
            for path in drivers
        }},
        "records": selected, "library_variants": variants, "path_map": path_map, "uncompressed_bytes": total,
        "complete": set(selected) == required, "missing_roles": sorted(required - set(selected)),
        "provenance": {name: git("rev-parse", "HEAD:" + name) for name in ("Cargo.lock", "LICENSE")},
        "max_input_bytes": MAX_INPUT_BYTES, "max_archive_bytes": MAX_ARCHIVE_BYTES,
        "workflow": {key: os.environ.get(key) for key in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB", "GITHUB_SHA")},
        "files": {},
    }
    for name, path in sorted(files.items()):
        info = path.stat()
        manifest["files"][name] = {"bytes": info.st_size, "sha256": digest(path), "mode": info.st_mode & 0o7777,
                                   "uid": info.st_uid, "gid": info.st_gid, "mtime_ns": info.st_mtime_ns}
    manifest_path = output.with_suffix(output.suffix + ".manifest.json")
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    with tarfile.open(output, "w:gz", compresslevel=1, format=tarfile.PAX_FORMAT, dereference=True) as archive:
        archive.add(manifest_path, arcname="manifest.json", recursive=False)
        for name, path in sorted(files.items()):
            info = archive.gettarinfo(str(path), arcname=name)
            ns = manifest["files"][name]["mtime_ns"]
            info.pax_headers["mtime"] = str(ns // 10**9) + "." + f"{ns % 10**9:09d}"
            with path.open("rb") as stream:
                archive.addfile(info, stream)
    if output.stat().st_size > MAX_ARCHIVE_BYTES:
        raise RuntimeError("native bundle exceeds the 1 GiB archive bound")
    output.with_suffix(output.suffix + ".sha256").write_text(digest(output) + "  " + output.name + "\n")
    print(f"Native replay bundle: {len(files)} files, {total} raw bytes; no test verdict changed")


if __name__ == "__main__":
    if len(sys.argv) in {4, 5} and sys.argv[1] == "pack":
        if sys.argv[2] not in {"workspace", "feature-off"}:
            raise SystemExit("unknown native bundle feature graph")
        if len(sys.argv) == 5 and sys.argv[4] != "--available":
            raise SystemExit("unknown native bundle option")
        pack(sys.argv[2], Path(sys.argv[3]), available=len(sys.argv) == 5)
    else:
        raise SystemExit(wrapper(sys.argv[1], sys.argv[2:]))
