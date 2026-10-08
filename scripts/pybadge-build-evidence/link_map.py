"""Prepared diagnostic relink/map capture; never run or disassemble firmware."""
import argparse
import hashlib
import json
from pathlib import Path
import shlex
import subprocess

from preprocess import compiler_from_record


def diagnostic_command(text, compiler, output, map_file):
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    if len(lines) != 1 or any(char in lines[0] for char in "$`\n"):
        raise ValueError("unsupported generated link command")
    command = shlex.split(lines[0])
    if not command or command[0] != compiler:
        raise ValueError("link/compiler identity mismatch")
    if any(token in (";", "&&", "||", "|", ">", "<") for token in command):
        raise ValueError("shell link recipes are unsupported")
    indexes = [i for i, token in enumerate(command) if token == "-o"]
    if len(indexes) != 1 or indexes[0] + 1 == len(command):
        raise ValueError("ambiguous original link output")
    if any(token.startswith("@") or "-Map" in token or token == "--cref" for token in command):
        raise ValueError("response files/existing map options require explicit review")
    at = indexes[0] + 1
    original = command[at]
    if original.startswith("-") or original == str(output):
        raise ValueError("invalid or colliding diagnostic output")
    command[at] = str(output)
    command.append("-Wl,-Map=" + str(map_file))
    return original, command


def file_digest(path):
    result = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def invoke_link(command, build):
    try:
        result = subprocess.run(command, cwd=build, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=120, check=False)
        return result.returncode, False, None, result.stdout, result.stderr
    except subprocess.TimeoutExpired as error:
        return None, True, None, error.stdout or b"", error.stderr or b""
    except OSError as error:
        return None, False, str(error), b"", b""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    root, out = args.root.resolve(), args.out.resolve()
    build = root / "build"
    out.mkdir(parents=True, exist_ok=True)
    command_path = build / "CMakeFiles/ITSYBITSY_M4.dir/link.txt"
    raw = command_path.read_bytes()
    # Preserve the observed recipe even when admission rejects it. A later
    # source correction must not replace the first unsupported recipe.
    with (out / "original-link.txt").open("xb") as capture:
        capture.write(raw)
    records = list((build / "CMakeFiles").glob("*/CMakeCXXCompiler.cmake"))
    if len(records) != 1:
        raise ValueError("ambiguous generated compiler record")
    compiler = compiler_from_record(records[0].read_text())
    diagnostic = build / "pybadge-map-diagnostic.elf"
    map_file = out / "diagnostic-link.map"
    report_path = out / "diagnostic-link-evidence.json"
    for path in (diagnostic, map_file, report_path):
        if path.exists():
            raise ValueError("diagnostic evidence already exists; refusing overwrite")
    original_name, command = diagnostic_command(raw.decode(), compiler, diagnostic, map_file)
    original = (build / original_name).resolve()
    if not original.is_relative_to(build) or not original.is_file() or original == diagnostic:
        raise ValueError("original ELF must be a distinct existing build output")
    before = file_digest(original)
    exit_code, timed_out, spawn_error, stdout, stderr = invoke_link(command, build)
    with (out / "diagnostic-link-stdout.txt").open("xb") as capture:
        capture.write(stdout)
    with (out / "diagnostic-link-stderr.txt").open("xb") as capture:
        capture.write(stderr)
    report = {
        "schema": 1, "originalLinkSha256": hashlib.sha256(raw).hexdigest(),
        "compilerSha256": file_digest(Path(compiler)), "exitCode": exit_code,
        "timedOut": timed_out, "spawnError": spawn_error,
        "originalElfSha256": before, "originalUnchanged": file_digest(original) == before,
        "diagnosticElfSha256": file_digest(diagnostic) if diagnostic.is_file() else None,
        "mapSha256": file_digest(map_file) if map_file.is_file() else None,
        "boundary": "Diagnostic relink adds map reporting and a separate output; not the original invocation, licence clearance or firmware execution",
    }
    with report_path.open("x") as capture:
        json.dump(report, capture, indent=2)
        capture.write("\n")
    if (exit_code != 0 or timed_out or spawn_error or not report["originalUnchanged"] or
            report["diagnosticElfSha256"] != before or not map_file.is_file() or
            not 0 < map_file.stat().st_size <= 8 * 1024 * 1024):
        raise ValueError("diagnostic relink/map identity failed; preserve first result")
    print("Diagnostic map captured; original and diagnostic ELF byte identity PASS")


if __name__ == "__main__":
    main()
