#!/usr/bin/env node

import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repository = path.resolve(process.argv[2] || projectRoot);
const config = path.join(projectRoot, '.gitleaks.toml');
const MAX_GIT_LIST_BYTES = 64 * 1024 * 1024;

function git(args) {
  const result = spawnSync('git', ['-C', repository, ...args], {
    encoding: 'utf8', maxBuffer: MAX_GIT_LIST_BYTES,
  });
  if (result.error || result.status !== 0) {
    // Git errors can include user content. Do not echo subprocess output.
    throw new Error(`git ${args[0]} failed; the scan is incomplete.`);
  }
  return result.stdout;
}

// Stream every reachable object, including deleted blobs, merge-only content,
// commit messages and annotated tags. A diff-only scan misses some of these.
async function snapshotHistory(destination, scratch, labels) {
  const ids = [...new Set(git(['rev-list', '--objects', '--all', '--no-object-names'])
    .trim().split('\n').filter(Boolean))];
  if (ids.some((id) => !/^[a-f0-9]{40,64}$/.test(id))) {
    throw new Error('Unexpected Git object list; the scan is incomplete.');
  }
  const listPath = path.join(scratch, 'object-list');
  fs.writeFileSync(listPath, ids.join('\n') + (ids.length ? '\n' : ''), { mode: 0o600 });
  const input = fs.openSync(listPath, 'r');
  const child = spawn('git', ['-C', repository, 'cat-file', '--batch'], {
    stdio: [input, 'pipe', 'ignore'],
  });
  fs.closeSync(input);
  const completion = once(child, 'close');
  // Attach a rejection handler immediately, including when stream parsing fails.
  completion.catch(() => {});
  let buffer = Buffer.alloc(0);
  let pending = null;
  let processed = 0;
  const counts = { blob: 0, commit: 0, tag: 0, tree: 0 };
  try {
    for await (const chunk of child.stdout) {
      buffer = Buffer.concat([buffer, chunk]);
      while (buffer.length) {
        if (!pending) {
          const newline = buffer.indexOf(10);
          if (newline < 0) break;
          const match = /^([a-f0-9]{40,64}) (blob|commit|tag|tree) (\d+)$/.exec(
            buffer.subarray(0, newline).toString('ascii'),
          );
          if (!match || match[1] !== ids[processed]) {
            throw new Error('Unexpected Git object header; the scan is incomplete.');
          }
          const [, id, type, size] = match;
          const name = `${id}.txt`;
          pending = {
            remaining: Number(size),
            fd: type === 'tree' ? null : fs.openSync(path.join(destination, name), 'wx', 0o600),
          };
          if (type !== 'tree') labels.set(name, `${type} ${id}`);
          counts[type] += 1;
          buffer = buffer.subarray(newline + 1);
        }
        const length = Math.min(pending.remaining, buffer.length);
        if (length && pending.fd !== null) {
          fs.writeFileSync(pending.fd, buffer.subarray(0, length));
        }
        pending.remaining -= length;
        buffer = buffer.subarray(length);
        if (pending.remaining || !buffer.length) break;
        if (buffer[0] !== 10) throw new Error('Invalid Git object boundary.');
        if (pending.fd !== null) fs.closeSync(pending.fd);
        pending = null;
        processed += 1;
        buffer = buffer.subarray(1);
      }
    }
    const [code] = await completion;
    if (code !== 0 || pending || buffer.length || processed !== ids.length) {
      throw new Error('Git object snapshot did not finish; the scan is incomplete.');
    }
    return counts;
  } finally {
    if (pending?.fd != null) fs.closeSync(pending.fd);
    if (child.exitCode === null) child.kill();
    await completion.catch(() => {});
  }
}

function snapshotWorktree(destination, labels) {
  const root = fs.realpathSync(repository);
  const files = [...new Set(git(['ls-files', '-z', '--cached', '--others', '--exclude-standard'])
    .split('\0').filter(Boolean))];
  let count = 0;
  for (const relative of files) {
    const source = path.resolve(root, relative);
    if (!source.startsWith(root + path.sep)) throw new Error('Invalid worktree path.');
    let metadata;
    try { metadata = fs.lstatSync(source); } catch (error) {
      if (error.code === 'ENOENT') continue; // A tracked file deleted in the worktree.
      throw error;
    }
    if (!metadata.isFile() && !metadata.isSymbolicLink()) {
      throw new Error('Unsupported worktree entry (including submodules); scan it separately.');
    }
    // Never traverse a symlinked parent into personal files outside the checkout.
    const parent = fs.realpathSync(path.dirname(source));
    if (parent !== root && !parent.startsWith(root + path.sep)) {
      throw new Error('Worktree path leaves the checkout; the scan is incomplete.');
    }
    const name = `worktree-${count++}.txt`;
    const target = path.join(destination, name);
    fs.writeFileSync(target, metadata.isSymbolicLink() ? fs.readlinkSync(source) : fs.readFileSync(source), {
      mode: 0o600, flag: 'wx',
    });
    labels.set(name, `worktree ${JSON.stringify(relative)}`);
  }
  return count;
}

async function main() {
  const version = spawnSync('gitleaks', ['version'], { encoding: 'utf8' });
  if (version.error || version.status !== 0) throw new Error('Install gitleaks first: brew install gitleaks');
  if (git(['rev-parse', '--is-shallow-repository']).trim() !== 'false') {
    throw new Error('Shallow history cannot be fully scanned. Fetch the complete history first.');
  }
  if (path.resolve(git(['rev-parse', '--show-toplevel']).trim()) !== fs.realpathSync(repository)) {
    throw new Error('Pass the root of a Git worktree.');
  }
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'wakenote-security-'));
  fs.chmodSync(scratch, 0o700);
  try {
    const snapshot = path.join(scratch, 'snapshot');
    fs.mkdirSync(snapshot, { mode: 0o700 });
    const labels = new Map();
    const counts = await snapshotHistory(snapshot, scratch, labels);
    const files = snapshotWorktree(snapshot, labels);
    console.log(`Scanning ${counts.blob} blobs, ${counts.commit} commits, ${counts.tag} tags and ${files} worktree files with gitleaks ${version.stdout.trim()}.`);
    const report = path.join(scratch, 'report.json');
    const ignore = path.join(scratch, 'empty-ignore');
    fs.writeFileSync(ignore, '', { mode: 0o600 });
    const scan = spawnSync('gitleaks', [
      'dir', snapshot, '--config', config, '--redact', '--no-banner', '--no-color',
      '--ignore-gitleaks-allow', '--gitleaks-ignore-path', ignore,
      '--max-decode-depth', '5', '--max-archive-depth', '2',
      '--report-format', 'json', '--report-path', report,
    ], { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
    if (scan.error || ![0, 1].includes(scan.status) || !fs.existsSync(report)) {
      throw new Error('Gitleaks did not finish. Check the installed tool version; the scan is incomplete.');
    }
    const findings = JSON.parse(fs.readFileSync(report, 'utf8'));
    if (!Array.isArray(findings)) throw new Error('Invalid Gitleaks report.');
    for (const finding of findings) {
      // Never print Match, Secret, source lines, or scanner stderr.
      const location = labels.get(path.basename(finding.File)) || 'nested decoded/archive content';
      console.error(`${finding.RuleID}: ${location}, line ${finding.StartLine}`);
    }
    if (findings.length || scan.status !== 0) {
      console.error(`Secret scan failed: ${findings.length} finding(s). No secret values were printed.`);
      process.exitCode = 1;
    } else {
      console.log('Secret scan passed: no findings. Ignored local files and unreachable objects are outside this scan.');
    }
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

main().catch(() => {
  // Exception details can contain file contents or personal paths.
  console.error('Security scan could not complete. Ensure gitleaks is installed and use a complete Git checkout with no submodules or external symlink parents.');
  process.exitCode = 2;
});
