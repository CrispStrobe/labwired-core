#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Hosted source build of the owned SAMD game, never an acquired firmware image.

The fresh result has its own identities. The earlier qualified campaign is a
source reference, not a qualification of this compiler's newly produced bytes.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import urllib.request
import zipfile

SOURCES = {
    "samd": ("CrispStrobe/codal-samd", "5d3f7e4a704006ef2acbe0be3dd0e59f9be1ef0a"),
    "core": ("lancaster-university/codal-core", "2ed19d64ec3359143409a4b0a9758b4aeef3ee7d"),
    "generator": ("lancaster-university/codal", "7cf09a3a37c8aca6e100b7265c81e6cdf6129dd2"),
    "board": ("lancaster-university/codal-itsybitsy-m4", "6ecd80ccf1abcc126d06653f178ca03a4d1c6421"),
}
INPUTS = {
    "arcade.tgz": ("https://registry.npmjs.org/pxt-arcade/-/pxt-arcade-4.2.1.tgz", "d403926da1c96dd71a696f0182dbdd82b97babb9d0de9281b8e2a6b67c57cf32"),
    "core.tgz": ("https://registry.npmjs.org/pxt-core/-/pxt-core-13.2.1.tgz", "e2c4c2a3f353f02abff1eff98ad066eab28e643311d0688961fb8b0388578033"),
    "dfp.atpack": ("https://packs.download.microchip.com/Microchip.SAMD51_DFP.3.9.276.atpack", "6f4e530bb8ce3d7a86b7bfafb072110f1e6eb4aa53e554c71f3db6a9720c039f"),
}


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def run(args, cwd=None):
    subprocess.run([str(x) for x in args], cwd=cwd, check=True)


def capture(args, cwd=None):
    return subprocess.check_output([str(x) for x in args], cwd=cwd, text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    a = parser.parse_args()
    need(os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_REPOSITORY") in {
        "CrispStrobe/labwired-core", "CrispStrobe/bw-board", "CrispStrobe/brickwright-lite"
    }, "build only in the owned hosted qualification workflows")
    work, out = a.work.resolve(), a.out.resolve()
    need(not work.exists() and not out.exists() and work != out and
         not work.is_relative_to(out) and not out.is_relative_to(work), "fresh disjoint work/output directories")
    work.mkdir(parents=True)
    out.parent.mkdir(parents=True, exist_ok=True)
    paths = {}
    for name, (repo, ref) in SOURCES.items():
        p = paths[name] = work / name
        run(["git", "init", "-q", p])
        run(["git", "fetch", "--depth=1", "https://github.com/" + repo + ".git", ref], p)
        run(["git", "checkout", "--detach", "FETCH_HEAD"], p)
        need(capture(["git", "rev-parse", "HEAD"], p) == ref, "exact public source checkout")
    samd = paths["samd"]
    for name, (url, digest) in INPUTS.items():
        with urllib.request.urlopen(url, timeout=120) as response:
            raw = response.read()
        need(sha(raw) == digest, "admitted archive bytes: " + name)
        (work / name).write_bytes(raw)
    for archive, member, dest, digest in [
        ("arcade.tgz", "package/built/target.json", "target.json", "6f35aadf1456436b3318d2f2339cc3d1add6e6c3b0bf8566f86d7f7bb4f7722e"),
        ("core.tgz", "package/built/web/pxtworker.js", "pxtworker.js", "1d9e73fd4bc0c0939b938395537ff1934b398654f9026f88e9293c32ea5acb0e"),
    ]:
        with tarfile.open(work / archive) as t:
            info = t.getmember(member)
            need(info.isfile(), "ordinary admitted payload member")
            raw = t.extractfile(info).read()
        need(sha(raw) == digest, "admitted compiler payload")
        (work / dest).write_bytes(raw)
    with zipfile.ZipFile(work / "dfp.atpack") as z:
        atdf = work / "ATSAMD51J19A.atdf"
        atdf.write_bytes(z.read("samd51a/atdf/ATSAMD51J19A.atdf"))
        need(z.read("LICENSE.txt") == (samd / "tools/samd51-device-header/notices/microchip-samd51-dfp-3.9.276-LICENSE.txt").read_bytes(), "reviewed Apache device-pack notice")
    node, python = "node", "python3"
    run([node, samd / "tools/pxt-native-request/request.mjs", work / "target.json", work / "pxtworker.js", "samd51adafruit", work / "request.json"], samd)
    need(sha((work / "request.json").read_bytes()) == "19efcdc73769fdfdeb51aa215c528bebad59782cbc538f72f4a194326f1f42b1", "original admitted build request")
    run([python, "-B", samd / "tools/samd51-device-header/generate.py", atdf, work / "device"], samd)
    run([python, "-B", samd / "tools/samd51-device-header/test_generate.py", atdf, work / "device"], samd)
    run([python, "-B", samd / "no-asf-board/fullboard/build.py", "--colour-profile", "rgb444-lut", "--core", paths["core"], "--generator", paths["generator"], "--board", paths["board"], "--request", work / "request.json", "--device", work / "device", "--out", work / "base"], samd)
    run([node, samd / "tools/pxt-native-request/compile.mjs", work / "target.json", work / "pxtworker.js", "samd51adafruit", work / "base/no-asf-board.hex", samd / "samd-game/game.ts", work / "program"], samd)
    run([python, "-B", samd / "no-asf-board/native/boot.py", work / "program/binary.uf2", work / "image.bin"], samd)
    run([python, "-B", samd / "samd-game/package.py", work / "image.bin", out, work / "base/no-asf-board.elf"], samd)
    for name in ["game.ts", "wasm.mjs"]:
        shutil.copyfile(samd / "samd-game" / name, out / name)
    notices = out / "notices"
    notices.mkdir()
    for name, root in paths.items():
        for source in root.iterdir():
            if source.is_file() and source.name.lower().startswith(("license", "copying")):
                shutil.copyfile(source, notices / (name + "-" + source.name))
    shutil.copyfile(samd / "tools/samd51-device-header/notices/microchip-samd51-dfp-3.9.276-LICENSE.txt", notices / "device-pack-LICENSE.txt")
    shutil.copyfile(samd / "no-asf-board/cf2/notices/uf2-samdx1-LICENSE", notices / "uf2-samdx1-LICENSE")
    for archive in ["arcade.tgz", "core.tgz"]:
        count = 0
        with tarfile.open(work / archive) as t:
            for info in t.getmembers():
                name = Path(info.name)
                if info.isfile() and name.name.lower().startswith(("license", "copying")):
                    need(not name.is_absolute() and ".." not in name.parts, "canonical archive notice name")
                    dest = notices / archive / name
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    dest.write_bytes(t.extractfile(info).read())
                    count += 1
        need(count > 0, "retain compiler package notice: " + archive)
    for package in ["gcc-arm-none-eabi", "gcc-13-base", "libnewlib-arm-none-eabi", "libnewlib-dev", "binutils-arm-none-eabi"]:
        source = Path("/usr/share/doc") / package / "copyright"
        need(source.is_file(), "retain selected compiler/library distribution notice: " + package)
        shutil.copyfile(source, notices / (package + "-copyright"))
    for name in ["COPYING.RUNTIME", "COPYING3"]:
        shutil.copyfile(samd / "runtime-services/reviewed-toolchain" / name, notices / name)
    shutil.copyfile("/usr/share/common-licenses/GPL-3", notices / "GPL-3")
    (out / "NOTICES.md").write_text("# Source-built SAMD game components\n\nOwned SAMD adaptations retain their upstream source attribution. The selected\nCODAL and board sources, Apache-2.0 generated device headers, PXT source payloads,\nand compiler/newlib components keep their individual notices. See notices/ and\nthe exact build report; GCC runtime components require the applicable library\nexception. No ASF4 or hardware-use-restricted implementation is admitted by the\nsource census. This manifest is not blanket legal clearance. No vendor firmware\nor bootloader code is included. Fresh guest execution remains unqualified until\nthe consuming workflow actually runs and checks these exact image bytes.\n")
    for src, dest in [(work / "base/build-report.json", "build-report.json"), (work / "base/commands.json", "commands.json"),
                      (work / "base/no-asf-board.map", "no-asf-board.map"), (work / "base/no-asf-board.elf", "base.elf"),
                      (work / "device/generator.json", "device-generator.json"), (work / "request.json", "request.json")]:
        shutil.copyfile(src, out / dest)
    shutil.copytree(work / "device", out / "generated-device")
    report = json.loads((work / "base/build-report.json").read_text())
    roots = {"request": [work / "base/request"], "cmsis": [work / "base/cmsis"],
             "device": [work / "device"], "core": [paths["core"]], "board": [paths["board"]],
             "support": [samd / "no-asf-board/support"], "samd": [samd / "inc", samd / "src"]}
    for name, digest in report["dependencies"].items():
        parts = Path(name).parts
        need(parts and not Path(name).is_absolute() and ".." not in parts and "\\" not in name, "canonical selected dependency")
        if name == "config":
            candidates = [work / "base/codal_extra_definitions.h"]
        elif name == "asm":
            candidates = [samd / "asm/CortexContextSwitch.s"]
        elif parts[0] == "toolchain":
            candidates = [Path(report["toolchain_search_roots"][int(parts[1])]).joinpath(*parts[2:])]
        else:
            need(parts[0] in roots, "known source census category")
            candidates = [root.joinpath(*parts[1:]) for root in roots[parts[0]]]
        admitted = []
        for source in candidates:
            if not source.is_file():
                continue
            raw = source.read_bytes()
            if name == "core/source/driver-models/CodalUSB.cpp":
                need(raw.count(b"    delete buf;\n") == 1, "retained exact source adaptation")
                raw = raw.replace(b"    delete buf;\n", b"    delete[] buf;\n")
            if sha(raw) == digest:
                admitted.append(raw)
        need(admitted and all(raw == admitted[0] for raw in admitted), "retain exact selected source: " + name)
        dest = out / "selected-source" / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(admitted[0])
    libraries = {}
    for item in report["image"]["archive_members"]:
        match = re.fullmatch(r"(libc_nano\.a|libm\.a|libgcc\.a)\(([^()/]+)\)", item)
        need(match is not None, "only reviewed compiler/C/math archive members")
        archive, member = match.groups()
        path = Path(capture(["arm-none-eabi-gcc", "-mcpu=cortex-m4", "-mthumb", "-mfloat-abi=softfp", "-mfpu=fpv4-sp-d16", "-print-file-name=" + archive]))
        need(path.is_file(), "actual selected runtime archive")
        raw = subprocess.check_output(["arm-none-eabi-ar", "p", str(path), member])
        entry = libraries.setdefault(archive, dict(sha256=sha(path.read_bytes()), bytes=path.stat().st_size, members={}))
        entry["members"][member] = dict(sha256=sha(raw), bytes=len(raw))
    (out / "selected-libraries.json").write_text(json.dumps(libraries, indent=2, sort_keys=True) + "\n")
    versions = {tool: capture([tool, "--version"]).splitlines()[0] for tool in ["arm-none-eabi-gcc", "arm-none-eabi-ld", "node", "python3"]}
    files = {p.relative_to(out).as_posix(): {"sha256": sha(p.read_bytes()), "bytes": p.stat().st_size} for p in sorted(out.rglob("*")) if p.is_file()}
    manifest = dict(schema=2, kind="source-built-samd-game", physical_capture=False,
                    actual_guest_qualified=False, bootloader_code=False,
                    sources={name: dict(repository=repo, source=ref) for name, (repo, ref) in SOURCES.items()},
                    inputs={name: dict(url=url, sha256=digest) for name, (url, digest) in INPUTS.items()},
                    qualified_reference=dict(repository="CrispStrobe/codal-samd", source="8e4781e3a455d616f48780bd70fc7b587e917a3c", workflow_run=40334385377, artifact_id=12660811276, artifact_sha256="3fab3e5b45476c75893bfe9b3ff6f508ec9d56dbe3a374000ac6f0e45ce786ea"),
                    builder=dict(source=capture(["git", "rev-parse", "HEAD"], Path(__file__).resolve().parents[2]), recipe_sha256=sha(Path(__file__).read_bytes()), versions=versions), files=files)
    (out / "MANIFEST.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print("SAMD_SOURCE_GAME_BUILT guest_qualified=false image_sha256=" + files["game.bin"]["sha256"])


if __name__ == "__main__":
    main()
