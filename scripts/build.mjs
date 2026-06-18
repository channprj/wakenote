#!/usr/bin/env node
// Single entry point used by package.json `build` and friends.
// Mirrors the markdowner build CLI: `pnpm build [debug|release] [app|dmg] [install] [open]`.
// When invoked with no args (e.g. from Tauri's beforeBuildCommand), only the
// frontend bundle is built (tsc + vite build).
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptDir, '..');
const tauriDir = path.join(projectRoot, 'src-tauri');
const APP_BUNDLE_NAME = 'WakeNote.app';
const LEGACY_BUNDLE_NAME = 'Sagwan.app';
const BUNDLE_IDS = ['com.chann.wakenote', 'com.chann.sagwan'];
const PROCESS_NAMES = ['wakenote', 'sagwan'];
const BINARY_NAME = 'wakenote';
// Cargo feature that bundles the in-process sherpa-onnx engine (Parakeet /
// SenseVoice). It links onnxruntime + sherpa-onnx dynamically, so shipped
// bundles must carry those dylibs — see `bundleSherpaRuntime`.
const SHERPA_FEATURE = 'asr-sherpa';

function usage() {
  console.log(`Usage:
  pnpm build
  pnpm build debug
  pnpm build release dmg
  pnpm build install [open]
  pnpm build debug install [open]

Options:
  debug, --debug       Build the Tauri debug bundle
  release, --release   Build the Tauri release bundle (default)
  app                  Package the macOS .app bundle (default from tauri.conf)
  dmg                  Package a macOS .dmg installer
  install              Install the resulting macOS .app bundle
  open, --open         Open the installed app after installation (alias: --launch)
  --no-build           Install an already-built bundle
  --path <dir>         Install destination (default: /Applications, override with WAKENOTE_INSTALL_PATH)
  -h, --help           Show this help message`);
}

function fail(message, exitCode = 1) {
  console.error(message);
  process.exit(exitCode);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? projectRoot,
    env: options.env ?? process.env,
    stdio: options.stdio ?? 'inherit',
  });

  if (result.error) {
    if (options.allowFailure) {
      return result;
    }
    fail(`error: failed to run '${command}': ${result.error.message}`);
  }

  if (result.status !== 0 && !options.allowFailure) {
    process.exit(result.status ?? 1);
  }

  return result;
}

function runOrThrow(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? projectRoot,
    env: options.env ?? process.env,
    stdio: options.stdio ?? 'inherit',
  });

  if (result.error) {
    throw new Error(`error: failed to run '${command}': ${result.error.message}`);
  }

  if (result.status !== 0) {
    throw new Error(`error: '${command}' exited with status ${result.status ?? 1}`);
  }

  return result;
}

function commandExists(command) {
  const result = spawnSync('sh', ['-c', `command -v "$1" >/dev/null 2>&1`, 'sh', command], {
    cwd: projectRoot,
    env: process.env,
    stdio: 'ignore',
  });
  return result.status === 0;
}

function requireCommands(commands) {
  for (const command of commands) {
    if (!commandExists(command)) {
      fail(`error: '${command}' is required but not found in PATH`);
    }
  }
}

function parseArgs(argv) {
  const options = {
    doBuild: true,
    install: false,
    installPath: process.env.WAKENOTE_INSTALL_PATH ?? '/Applications',
    mode: 'release',
    open: false,
    bundle: null,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];

    if (arg === '-h' || arg === '--help' || arg === 'help') {
      usage();
      process.exit(0);
    }

    if (arg === 'debug' || arg === '--debug') {
      options.mode = 'debug';
    } else if (arg === 'release' || arg === '--release') {
      options.mode = 'release';
    } else if (arg === 'app') {
      options.bundle = 'app';
    } else if (arg === 'dmg') {
      options.bundle = 'dmg';
    } else if (arg === 'install') {
      options.install = true;
    } else if (arg === 'open' || arg === '--open' || arg === '--launch') {
      options.open = true;
      options.install = true;
    } else if (arg === 'no-build' || arg === '--no-build') {
      options.doBuild = false;
      options.install = true;
    } else if (arg === '--path') {
      const value = argv[index + 1];
      if (!value) {
        fail('error: --path requires a directory', 2);
      }
      options.installPath = value;
      index += 1;
    } else if (arg.startsWith('--path=')) {
      options.installPath = arg.slice('--path='.length);
    } else {
      fail(`error: unknown argument: ${arg}`, 2);
    }
  }

  return options;
}

function ensureDependencies() {
  requireCommands(['pnpm', 'cargo']);

  if (!fs.existsSync(path.join(projectRoot, 'node_modules'))) {
    console.log('==> Installing JS dependencies (pnpm install)');
    run('pnpm', ['install']);
  }
}

function buildFrontend() {
  run('pnpm', ['exec', 'tsc']);
  run('pnpm', ['exec', 'vite', 'build']);
}

function buildTauri(mode, bundle, env = process.env) {
  ensureDependencies();

  const bundleArgs = bundle ? ['--bundles', bundle] : [];
  // Bundle the in-process sherpa-onnx engine (Parakeet / SenseVoice) in shipped
  // builds. Kept out of the crate's default features so plain `cargo` builds and
  // CI stay light.
  const featureArgs = ['--features', SHERPA_FEATURE];

  if (mode === 'debug') {
    console.log('==> Building Tauri app (debug)');
    run('pnpm', ['tauri', 'build', '--debug', ...featureArgs, ...bundleArgs], { env });
  } else {
    console.log('==> Building Tauri app (release)');
    run('pnpm', ['tauri', 'build', ...featureArgs, ...bundleArgs], { env });
  }

  // Seal the freshly built bundle. See `sealBundleSignature` for why this
  // is necessary; doing it here means `pnpm build` alone (no install) also
  // yields a launchable bundle, and `ditto` later preserves the signature.
  const appBundle = path.join(
    tauriDir,
    'target',
    bundleDirForMode(mode),
    'bundle',
    'macos',
    APP_BUNDLE_NAME,
  );
  if (fs.existsSync(appBundle)) {
    bundleSherpaRuntime(appBundle, mode);
    sealBundleSignature(appBundle, false);
  }

  // `tauri build --bundles dmg` creates the DMG before this script regains
  // control, and the Tauri bundler may remove its temporary `.app` afterwards.
  // Repair the already-created image so `pnpm build release dmg` ships the same
  // self-contained app as the loose bundle above.
  if (bundle === 'dmg') {
    repairSherpaRuntimeInDmg(mode);
  }
}

// Re-sign a macOS .app bundle with an adhoc signature that seals the bundle
// resources. Tauri 2 + recent rustc emit a Mach-O carrying a `linker-signed`
// adhoc signature but never run `codesign` on the assembled bundle, so the
// binary advertises deep-signing semantics while the bundle has no
// `_CodeSignature/CodeResources` manifest. macOS taskgated treats that
// mismatch as a tampered bundle and SIGKILLs the process at exec with
// `CODESIGNING / Taskgated Invalid Signature`. Forcing a fresh adhoc bundle
// signature populates `Sealed Resources` and makes verification pass.
// Adhoc (`--sign -`) keeps this dependency-free — no developer identity.
function sealBundleSignature(appBundle, useSudo) {
  console.log('==> Sealing bundle signature (adhoc)');
  runMaybeSudo(useSudo, 'codesign', ['--force', '--deep', '--sign', '-', appBundle]);
}

// Copy the dylibs the `asr-sherpa` feature links dynamically into the app
// bundle and point the binary at them. sherpa-rs (`download-binaries`) links
// the binary against `@rpath/libonnxruntime.<ver>.dylib` and
// `@rpath/libsherpa-onnx-c-api.dylib` but bakes in NO `LC_RPATH`; cargo only
// resolves these via DYLD_FALLBACK_LIBRARY_PATH during dev. A standalone
// `.app` therefore fails dyld resolution at launch unless we both bundle the
// dylibs and add an rpath. Must run BEFORE codesigning: `install_name_tool`
// invalidates the Mach-O signature, which the subsequent adhoc seal restores.
function bundleSherpaRuntime(appBundle, mode) {
  const profileDir = path.join(tauriDir, 'target', bundleDirForMode(mode));
  const binary = path.join(appBundle, 'Contents', 'MacOS', BINARY_NAME);
  const frameworks = path.join(appBundle, 'Contents', 'Frameworks');

  if (!fs.existsSync(binary)) {
    fail(`error: bundle binary not found: ${binary}`);
  }

  // sherpa-rs-sys copies the prebuilt dylibs to the profile root (and deps/).
  const locate = (name) => {
    for (const dir of [profileDir, path.join(profileDir, 'deps')]) {
      const candidate = path.join(dir, name);
      if (fs.existsSync(candidate)) {
        return candidate;
      }
    }
    return null;
  };

  // Referenced by the linked binary / C API (otool -L). The unversioned
  // onnxruntime alias and the C++ wrapper are copied when present but are not
  // required by the current load chain.
  const required = ['libonnxruntime.1.17.1.dylib', 'libsherpa-onnx-c-api.dylib'];
  const optional = ['libonnxruntime.dylib', 'libsherpa-onnx-cxx-api.dylib'];

  const missing = required.filter((name) => !locate(name));
  if (missing.length > 0) {
    fail(`error: ${SHERPA_FEATURE} build is missing required dylibs: ${missing.join(', ')}
       searched ${profileDir} and its deps/. The shipped bundle would crash at launch.`);
  }

  console.log('==> Bundling sherpa-onnx runtime into Contents/Frameworks');
  fs.mkdirSync(frameworks, { recursive: true });
  for (const name of [...required, ...optional]) {
    const source = locate(name);
    if (source) {
      fs.copyFileSync(source, path.join(frameworks, name));
    }
  }

  // The binary resolves its `@rpath/*` deps from the bundled Frameworks; the
  // sherpa C API resolves its sibling onnxruntime from its own directory.
  addRpathIfMissing(binary, '@executable_path/../Frameworks');
  addRpathIfMissing(path.join(frameworks, 'libsherpa-onnx-c-api.dylib'), '@loader_path');
}

function latestDmgForMode(mode) {
  const dmgDir = path.join(tauriDir, 'target', bundleDirForMode(mode), 'bundle', 'dmg');
  if (!fs.existsSync(dmgDir)) {
    fail(`error: DMG output directory not found: ${dmgDir}`);
  }

  const dmgs = fs
    .readdirSync(dmgDir)
    .filter((name) => name.endsWith('.dmg'))
    .map((name) => {
      const fullPath = path.join(dmgDir, name);
      return { fullPath, mtimeMs: fs.statSync(fullPath).mtimeMs };
    })
    .sort((a, b) => b.mtimeMs - a.mtimeMs);

  if (dmgs.length === 0) {
    fail(`error: no DMG files found in ${dmgDir}`);
  }

  return dmgs[0].fullPath;
}

function repairSherpaRuntimeInDmg(mode) {
  const sourceDmg = latestDmgForMode(mode);
  const tempDir = fs.mkdtempSync(path.join(path.dirname(sourceDmg), '.wakenote-dmg-'));
  const mountRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'wakenote-dmg-mount-'));
  const mountPoint = path.join(mountRoot, 'mount');
  const rwBase = path.join(tempDir, 'image-rw');
  const rwDmg = `${rwBase}.dmg`;
  const repairedBase = path.join(tempDir, 'image-repaired');
  const repairedDmg = `${repairedBase}.dmg`;
  let attached = false;
  let failure = null;

  console.log('==> Repairing sherpa-onnx runtime inside DMG');
  fs.mkdirSync(mountPoint);

  try {
    runOrThrow('hdiutil', ['convert', sourceDmg, '-format', 'UDRW', '-o', rwBase]);
    // Tauri sized the image before the sherpa/onnxruntime dylibs were added.
    // Grow the temporary read-write image so the repaired app fits, then the
    // final UDZO conversion compresses it back down.
    runOrThrow('hdiutil', ['resize', '-size', '320m', rwDmg]);
    runOrThrow('hdiutil', ['attach', '-nobrowse', '-readwrite', '-mountpoint', mountPoint, rwDmg]);
    attached = true;

    const appBundle = path.join(mountPoint, APP_BUNDLE_NAME);
    if (!fs.existsSync(appBundle)) {
      fail(`error: DMG does not contain ${APP_BUNDLE_NAME}`);
    }

    bundleSherpaRuntime(appBundle, mode);
    sealBundleSignature(appBundle, false);

    runOrThrow('hdiutil', ['detach', mountPoint]);
    attached = false;
    runOrThrow('hdiutil', [
      'convert',
      rwDmg,
      '-format',
      'UDZO',
      '-imagekey',
      'zlib-level=9',
      '-o',
      repairedBase,
    ]);

    const originalDmg = path.join(tempDir, 'image-original.dmg');
    fs.renameSync(sourceDmg, originalDmg);
    fs.renameSync(repairedDmg, sourceDmg);
    fs.rmSync(originalDmg);
  } catch (error) {
    failure = error;
  } finally {
    if (attached) {
      spawnSync('hdiutil', ['detach', mountPoint], { stdio: 'inherit' });
    }
    fs.rmSync(tempDir, { recursive: true, force: true });
    fs.rmSync(mountRoot, { recursive: true, force: true });
  }

  if (failure) {
    fail(failure.message);
  }
}

// `install_name_tool -add_rpath` fails if the path already exists, so add it
// only when `otool -l` does not already list it (keeps reruns idempotent).
function addRpathIfMissing(machO, rpath) {
  const listed = spawnSync('otool', ['-l', machO], { cwd: projectRoot, encoding: 'utf8' });
  if (listed.status === 0 && listed.stdout.includes(`path ${rpath} `)) {
    return;
  }
  run('install_name_tool', ['-add_rpath', rpath, machO]);
}

function ensureMacOsInstallTarget() {
  const result = spawnSync('uname', ['-s'], {
    cwd: projectRoot,
    env: process.env,
    encoding: 'utf8',
  });

  if (result.status !== 0 || result.stdout.trim() !== 'Darwin') {
    fail('error: pnpm build install currently supports macOS only');
  }
}

function canWrite(directory) {
  try {
    fs.accessSync(directory, fs.constants.W_OK);
    return true;
  } catch {
    return false;
  }
}

function runMaybeSudo(useSudo, command, args, options = {}) {
  if (useSudo) {
    return run('sudo', [command, ...args], options);
  }
  return run(command, args, options);
}

function isWakeNoteRunning() {
  for (const name of PROCESS_NAMES) {
    const result = spawnSync('pgrep', ['-x', name], { stdio: 'ignore' });
    if (result.status === 0) {
      return true;
    }
  }
  return false;
}

function quitRunningWakeNote() {
  if (!isWakeNoteRunning()) {
    return;
  }

  console.log('==> Quitting running WakeNote');
  for (const id of BUNDLE_IDS) {
    spawnSync('osascript', ['-e', `tell application id "${id}" to quit`], { stdio: 'ignore' });
  }

  for (let i = 0; i < 20; i += 1) {
    if (!isWakeNoteRunning()) {
      return;
    }
    spawnSync('sh', ['-c', 'sleep 0.5']);
  }

  fail('error: WakeNote is still running. Quit it and rerun this command.');
}

function resolveInstallPath(raw) {
  const expanded = raw.startsWith('~') ? path.join(process.env.HOME ?? '', raw.slice(1)) : raw;
  return path.resolve(projectRoot, expanded);
}

function bundleDirForMode(mode) {
  return mode === 'debug' ? 'debug' : 'release';
}

function installBundle(options) {
  ensureMacOsInstallTarget();

  const appBundle = path.join(
    tauriDir,
    'target',
    bundleDirForMode(options.mode),
    'bundle',
    'macos',
    APP_BUNDLE_NAME,
  );

  if (!fs.existsSync(appBundle)) {
    fail(`error: bundle not found: ${appBundle}
       run without --no-build, or build the app first.`);
  }

  const installPath = resolveInstallPath(options.installPath);

  if (!fs.existsSync(installPath)) {
    console.log(`==> Creating install directory: ${installPath}`);
    fs.mkdirSync(installPath, { recursive: true });
  }

  const dest = path.join(installPath, APP_BUNDLE_NAME);
  const legacyDest = path.join(installPath, LEGACY_BUNDLE_NAME);

  quitRunningWakeNote();

  let useSudo = false;
  if (!canWrite(installPath)) {
    if (!commandExists('sudo')) {
      fail(`error: install path '${installPath}' is not writable and sudo is unavailable`);
    }
    console.log(`==> ${installPath} is not writable; using sudo for install`);
    useSudo = true;
  }

  if (fs.existsSync(dest)) {
    console.log(`==> Removing existing bundle at ${dest}`);
    runMaybeSudo(useSudo, 'rm', ['-rf', dest]);
  }

  if (fs.existsSync(legacyDest)) {
    console.log(`==> Removing legacy bundle at ${legacyDest}`);
    runMaybeSudo(useSudo, 'rm', ['-rf', legacyDest]);
  }

  console.log(`==> Installing to ${dest}`);
  // Invoke macOS `ditto` by absolute path: a bare `ditto` resolves through
  // PATH, where an unrelated tool of the same name may shadow the system one.
  runMaybeSudo(useSudo, '/usr/bin/ditto', [appBundle, dest]);
  runMaybeSudo(useSudo, 'xattr', ['-dr', 'com.apple.quarantine', dest], {
    allowFailure: true,
    stdio: 'ignore',
  });

  // `ditto` preserves the source signature, but re-seal at the install
  // destination too: this covers `--no-build` (installing a pre-built or
  // externally-produced bundle that may be unsigned) and guards against any
  // attribute drift introduced by the copy + quarantine strip above.
  sealBundleSignature(dest, useSudo);

  console.log(`==> Done. Installed: ${dest}`);

  if (options.open) {
    console.log(`==> Opening ${dest}`);
    run('open', [dest]);
  } else {
    console.log(`    Launch with: open '${dest}'`);
  }
}

const argv = process.argv.slice(2).filter((arg) => arg !== '--');

if (argv.length === 0) {
  buildFrontend();
  process.exit(0);
}

const options = parseArgs(argv);

if (options.doBuild) {
  buildTauri(options.mode, options.bundle);
}

if (options.install) {
  installBundle(options);
}
