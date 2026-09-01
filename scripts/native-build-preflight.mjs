#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

export function ensureNativeBuildTools({
  env = process.env,
  platform = process.platform,
  run = spawnSync,
} = {}) {
  const cmakeCommand = env.CMAKE?.trim() || 'cmake';
  const result = run(cmakeCommand, ['--version'], {
    env,
    stdio: 'ignore',
  });

  if (!result.error && result.status === 0) {
    return;
  }

  const installHint =
    platform === 'darwin'
      ? "Install it with Homebrew: 'brew install cmake'."
      : "Install CMake with your system package manager and ensure it is in PATH.";
  const configuredCommand = env.CMAKE?.trim()
    ? ` The configured CMAKE executable is '${cmakeCommand}'.`
    : '';

  throw new Error(
    `CMake is required to compile the bundled Whisper runtime but could not be executed.${configuredCommand}\n` +
      `${installHint}\n` +
      "Verify the installation with: 'cmake --version'.",
  );
}

function main() {
  try {
    ensureNativeBuildTools();
  } catch (error) {
    console.error(`error: ${error.message}`);
    process.exit(1);
  }
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : null;
if (invokedPath === fileURLToPath(import.meta.url)) {
  main();
}
