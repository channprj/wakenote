#!/usr/bin/env node
// Single entry point used by package.json `build` and friends.
// Mirrors the markdowner build CLI: `pnpm build [debug|release] [install] [open]`.
// When invoked with no args (e.g. from Tauri's beforeBuildCommand), only the
// frontend bundle is built (tsc + vite build).
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
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

function usage() {
  console.log(`Usage:
  pnpm build
  pnpm build debug
  pnpm build install [open]
  pnpm build debug install [open]

Options:
  debug, --debug       Build the Tauri debug bundle
  release, --release   Build the Tauri release bundle (default)
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

function buildTauri(mode, env = process.env) {
  ensureDependencies();

  if (mode === 'debug') {
    console.log('==> Building Tauri app (debug)');
    run('pnpm', ['tauri', 'build', '--debug'], { env });
  } else {
    console.log('==> Building Tauri app (release)');
    run('pnpm', ['tauri', 'build'], { env });
  }
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
  runMaybeSudo(useSudo, 'ditto', [appBundle, dest]);
  runMaybeSudo(useSudo, 'xattr', ['-dr', 'com.apple.quarantine', dest], {
    allowFailure: true,
    stdio: 'ignore',
  });

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
  buildTauri(options.mode);
}

if (options.install) {
  installBundle(options);
}
