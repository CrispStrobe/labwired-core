// Separate diagnostic copy of immutable bw-board builder d1f2c8ebfc5e4e76d5fc81f930588c13de13e974.
// Ordinary builder/harness are never edited. No publication. Hosted only.
if (process.env.GITHUB_ACTIONS !== 'true') throw Error('Hosted engine builds only');
/**
 * Build the labwired-wasm artifact the boundary-A adapter runs on.
 *
 * Upstream ships per-platform CLI binaries and nothing on npm, so there is no
 * published wasm to depend on: whoever wants labwired in a browser has to
 * produce it. This makes that one command, and pins the two things that make
 * it fail in ways the error message does not explain.
 *
 *   node scripts/build-labwired-wasm.mjs [--ref <sha>] [--out <dir>]
 *                            [--target-dir <dir>] [--work-dir <dir>] [--keep]
 *
 * `--target-dir` persists the cargo cache between runs. Without it every
 * invocation gets a fresh temp dir and recompiles the whole workspace — about
 * ten minutes, and the reason the first two runs here were slow.
 *
 * Output (in --out, default ./build/labwired-wasm):
 *   nodejs/   glue for `node --test` — CommonJS, loads the wasm with readFileSync
 *   web/      glue for a browser bundle — ESM with an explicit `init(url)`
 *   BUILD-INFO.json  ref, sizes (raw / stripped / brotli), sha256s, per target
 *
 * TWO TARGETS, because one does not fit both consumers. The `nodejs` glue
 * `require()`s and reads the wasm off disk; drop that in a browser bundle and
 * it breaks at build time. `web` was chosen over `bundler` deliberately: the
 * bundler target hands webpack a wasm IMPORT, which needs
 * `experiments.asyncWebAssembly` turned on, while `web` fetches the module from
 * a URL at runtime — exactly how the emu8051 artifact already loads, so it
 * needs no webpack change at all.
 *
 * TWO PINS THAT ARE NOT OPTIONAL
 * ------------------------------
 * 1. The wasm-bindgen CLI version must EXACTLY match the `wasm-bindgen` crate
 *    version in the built artifact — not the version in Cargo.toml, the one
 *    Cargo.lock resolved. Mismatch fails with "schema version" and no hint as
 *    to which side to move. This script reads the resolved version out of the
 *    lockfile and installs that CLI into a scratch --root, so the caller's
 *    global wasm-bindgen is never touched and the build is reproducible on a
 *    machine that has a different one.
 * 2. The build must come from a PINNED ref of the fork, for the same reason
 *    every other vendored thing here is pinned.
 *
 * DETERMINISM, MEASURED IN THREE ROUNDS
 * -------------------------------------
 * Round 1: two builds at different temp paths gave an identical
 * `labwired_wasm.js` and a DIFFERENT `labwired_wasm_bg.wasm`. The workspace
 * sets `debug = true`, and the build path was embedded in the artifact.
 *
 * Round 2: `--remap-path-prefix` for the source, target and cargo home — and
 * it STILL differed. On macOS `tmpdir()` is `/var/folders/...` while the
 * compiler emits the realpath `/private/var/folders/...`, so a prefix built
 * from tmpdir() alone matched nothing. Both forms are remapped now.
 *
 * Round 3: with that fixed the two artifacts differed in exactly FIVE
 * contiguous bytes out of 21,167,728, at the same size — the PID in
 * `.../labwired-wasm-<pid>/src/crates/coreroms/esp32s3/esp32s3_drom.bin`.
 * `--remap-path-prefix` rewrites what rustc emits; it does not reach a path a
 * BUILD SCRIPT baked into generated code. So the work directory is stable by
 * default rather than per-process, which removes the last varying input.
 *
 * What that buys and does not buy: two builds on this machine are
 * byte-identical, so BUILD-INFO.json's sha256 is checkable. Byte-identity on
 * ANOTHER machine additionally requires the same work-dir path — pass
 * `--work-dir` to match. Note the sha256 in lite's sync-labwired-wasm.mjs does
 * not depend on any of this: it authenticates the bytes that were published.
 * Determinism is what lets somebody else re-derive them.
 *
 * SIZES, MEASURED RATHER THAN QUOTED
 * ----------------------------------
 * The raw artifact is large because the workspace sets `debug = true`;
 * wasm-bindgen drops the custom sections. BUILD-INFO.json records raw,
 * post-bindgen and brotli sizes so a distribution decision (commit the
 * artifact? publish a release asset? not ship it yet?) is made against numbers
 * from this machine rather than a number someone remembers.
 */
import { execFileSync } from 'node:child_process';
import { mkdirSync, rmSync, existsSync, readFileSync, readdirSync, writeFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';
import { brotliCompressSync, constants as zlibConstants } from 'node:zlib';

const REPO = 'https://github.com/CrispStrobe/labwired-core.git';
/** The fork's tested main, including the guarded Cortex-M, RISC-V and Xtensa
 * hot-loop paths. The fleet builds from this immutable source commit. */
const PIN = 'a7c7cbdfac7988ffc11a1d3fa1f5af2ecce57e18';

const arg = (name, dflt) => {
    const i = process.argv.indexOf(`--${name}`);
    return i === -1 ? dflt : process.argv[i + 1];
};
const ref = arg('ref', PIN);
const outDir = resolve(arg('out', join(process.cwd(), 'build', 'labwired-wasm')));
const targetDirArg = arg('target-dir', null);
const keep = process.argv.includes('--keep');

const run = (cmd, args, opts = {}) =>
    execFileSync(cmd, args, { stdio: 'inherit', ...opts });
const capture = (cmd, args, opts = {}) =>
    execFileSync(cmd, args, { encoding: 'utf8', ...opts }).trim();

// STABLE, not per-process: see DETERMINISM above. A build script embeds this
// path in the artifact, so a PID in it makes every build a different file.
const work = resolve(arg('work-dir', join(tmpdir(), 'labwired-wasm-build')));
const src = join(work, 'src');
const target = targetDirArg ? resolve(targetDirArg) : join(work, 'target');
const cliRoot = join(work, 'wb');

console.log(`labwired-wasm: building ${ref}`);
// The work dir is a fixed path now, so a leftover from a previous run would be
// silently reused as "the source" — including a different ref's checkout.
if (existsSync(work)) rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
try {
    // 1. the pinned source. Shallow, but a sha needs the full history to be
    //    fetchable by --branch, so clone the branch and check the sha out.
    run('git', ['clone', '--quiet', '--depth', '50', '--branch', 'main', REPO, src]);
    // A ref that is not in main's recent history (a commit on a branch, built
    // to validate an engine change before it lands) is fetched by sha: GitHub
    // serves any commit reachable from a ref. Without this the build died on
    // `fatal: unable to read tree` for everything but main.
    try {
        run('git', ['checkout', '--quiet', ref], { cwd: src });
    } catch {
        run('git', ['fetch', '--quiet', '--depth', '50', 'origin', ref], { cwd: src });
        run('git', ['checkout', '--quiet', ref], { cwd: src });
    }
    const head = capture('git', ['rev-parse', 'HEAD'], { cwd: src });
    console.log(`  source at ${head}`);

    // 2. the wasm target FOR LABWIRED'S OWN TOOLCHAIN. Their
    //    rust-toolchain.toml pins channel 1.95.0 and deliberately lists no
    //    `targets` (the cross-compile jobs add what they need), so the target
    //    must be added from INSIDE the checkout, where rustup's
    //    nearest-ancestor lookup finds that file. Installing wasm32 for
    //    `stable` in a CI step does nothing for this build: the first CI run
    //    died with `can't find crate for core` doing exactly that, and it only
    //    worked on the machine that wrote this because that machine already
    //    had wasm32 for 1.95.0 from unrelated work.
    run('rustup', ['target', 'add', 'wasm32-unknown-unknown'], { cwd: src });

    // 3. the wasm itself
    // Remap every absolute path that would otherwise be baked into the
    // artifact by `debug = true`. Without this the output is a fingerprint of
    // the machine that built it.
    const cargoHome = process.env.CARGO_HOME
        || join(process.env.HOME || '', '.cargo');
    // BOTH the path as given and its realpath. On macOS `tmpdir()` is
    // `/var/folders/...` while the compiler emits `/private/var/folders/...`
    // through the /var symlink, so a prefix built from tmpdir() alone matches
    // nothing — which is exactly how one `include_bytes!`d ROM path survived
    // the first remap and kept the artifact machine-specific. $HOME/.cargo is
    // a symlink here too (offloaded to external storage), same trap.
    const both = (dir) => {
        const out = [dir];
        try { const real = realpathSync(dir); if (real !== dir) out.push(real); } catch { /* may not exist yet */ }
        return out;
    };
    const rustflags = [
        ...both(src).map((d) => `--remap-path-prefix=${d}=/labwired`),
        ...both(target).map((d) => `--remap-path-prefix=${d}=/labwired-target`),
        ...both(cargoHome).map((d) => `--remap-path-prefix=${d}=/cargo`),
        process.env.RUSTFLAGS || '',
    ].join(' ').trim();
    run('cargo', ['build', '--release', '--target', 'wasm32-unknown-unknown', '-p', 'labwired-wasm',
        ...(process.argv.includes('--diagnostic') ? ['--features', 't16-discovery-census'] : [])],
        { cwd: src, env: { ...process.env, CARGO_TARGET_DIR: target, RUSTFLAGS: rustflags } });
    const rawPath = join(target, 'wasm32-unknown-unknown', 'release', 'labwired_wasm.wasm');
    const rawSize = statSync(rawPath).size;
    console.log(`  raw artifact ${(rawSize / 1048576).toFixed(1)} MB`);

    // 4. the MATCHING bindgen CLI, resolved from the lockfile. See the header:
    //    Cargo.toml says "0.2.92", the lock resolves something newer, and the
    //    CLI must match what was actually linked.
    const lock = readFileSync(join(src, 'Cargo.lock'), 'utf8');
    const m = lock.match(/name = "wasm-bindgen"\nversion = "([^"]+)"/);
    if (!m) throw new Error('could not find the resolved wasm-bindgen version in Cargo.lock');
    const wbVersion = m[1];
    console.log(`  wasm-bindgen resolved to ${wbVersion}; installing that CLI`);
    run('cargo', ['install', 'wasm-bindgen-cli', '--version', wbVersion, '--root', cliRoot, '--quiet'],
        { env: { ...process.env, CARGO_TARGET_DIR: join(work, 'wb-target') } });

    // 5. bindings, one set per consumer — see the header for why both.
    mkdirSync(outDir, { recursive: true });
    const TARGETS = ['nodejs', 'web'];
    for (const t of TARGETS) {
        mkdirSync(join(outDir, t), { recursive: true });
        run(join(cliRoot, 'bin', 'wasm-bindgen'),
            ['--target', t, '--out-dir', join(outDir, t), rawPath]);
    }

    // 6. what actually came out
    const info = { diagnosticOnly: true, parentTool: 'd1f2c8ebfc5e4e76d5fc81f930588c13de13e974', features: process.argv.includes('--diagnostic') ? ['t16-discovery-census'] : [], ref: head, wasmBindgen: wbVersion, rustflags, builtAt: new Date().toISOString(), targets: {} };
    info.rawBytes = rawSize;
    for (const t of TARGETS) {
        info.targets[t] = {};
        for (const name of readdirSync(join(outDir, t))) {
            if (!/\.(js|wasm)$/.test(name)) continue;   // skip the .d.ts
            const buf = readFileSync(join(outDir, t, name));
            info.targets[t][name] = {
                bytes: buf.length,
                brotliBytes: brotliCompressSync(buf, {
                    params: { [zlibConstants.BROTLI_PARAM_QUALITY]: 11 },
                }).length,
                sha256: createHash('sha256').update(buf).digest('hex'),
            };
        }
    }
    writeFileSync(join(outDir, 'BUILD-INFO.json'), `${JSON.stringify(info, null, 2)}\n`);

    console.log(`\nlabwired-wasm: wrote ${outDir}`);
    for (const [t, files] of Object.entries(info.targets)) {
        console.log(`  [${t}]`);
        for (const [name, f] of Object.entries(files)) {
            console.log(`    ${name.padEnd(24)} ${(f.bytes / 1048576).toFixed(2)} MB` +
                `  brotli ${(f.brotliBytes / 1048576).toFixed(2)} MB  sha256 ${f.sha256.slice(0, 16)}…`);
        }
        const br = Object.values(files).reduce((n, f) => n + f.brotliBytes, 0);
        console.log(`    served (brotli) ${(br / 1048576).toFixed(2)} MB`);
    }
} finally {
    if (keep) console.log(`\n(kept the work tree at ${work})`);
    else rmSync(work, { recursive: true, force: true });
}
