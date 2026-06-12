#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptDir, '..');
const dmgDir = path.join(projectRoot, 'src-tauri', 'target', 'release', 'bundle', 'dmg');
const downloadsDir = path.join(process.env.HOME ?? '', 'Downloads');

function fail(message, exitCode = 1) {
  console.error(message);
  process.exit(exitCode);
}

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: projectRoot,
    env: process.env,
    stdio: 'inherit',
  });

  if (result.error) {
    fail(`error: failed to run '${command}': ${result.error.message}`);
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function latestDmg() {
  if (!fs.existsSync(dmgDir)) {
    fail(`error: DMG output directory not found: ${dmgDir}`);
  }

  const dmgs = fs
    .readdirSync(dmgDir)
    .filter((name) => name.endsWith('.dmg'))
    .map((name) => {
      const fullPath = path.join(dmgDir, name);
      return { name, fullPath, mtimeMs: fs.statSync(fullPath).mtimeMs };
    })
    .sort((a, b) => b.mtimeMs - a.mtimeMs);

  if (dmgs.length === 0) {
    fail(`error: no DMG files found in ${dmgDir}`);
  }

  return dmgs[0];
}

run('pnpm', ['build', 'release', 'dmg']);

if (!fs.existsSync(downloadsDir)) {
  fs.mkdirSync(downloadsDir, { recursive: true });
}

const source = latestDmg();
const destination = path.join(downloadsDir, source.name);

if (fs.existsSync(destination)) {
  fs.rmSync(destination);
}

fs.renameSync(source.fullPath, destination);
console.log(`==> Moved DMG to ${destination}`);
