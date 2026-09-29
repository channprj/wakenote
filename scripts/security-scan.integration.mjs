import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const scanner = fileURLToPath(new URL('./security-scan.mjs', import.meta.url));

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'wakenote-scan-test-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const git = (...args) => {
    const result = spawnSync('git', ['-C', root, ...args], { encoding: 'utf8' });
    assert.equal(result.status, 0, 'Git fixture setup must succeed');
  };
  git('init', '-q');
  git('config', 'user.name', 'Scanner Test');
  git('config', 'user.email', 'scanner@example.com');
  git('config', 'commit.gpgsign', 'false');
  git('config', 'tag.gpgsign', 'false');
  fs.writeFileSync(path.join(root, 'clean.txt'), 'Public test content.\n');
  git('add', 'clean.txt');
  git('commit', '-qm', 'Initial fixture');
  const scan = () => spawnSync(process.execPath, [scanner, root], { encoding: 'utf8' });
  return { root, git, scan };
}

test('a clean complete repository passes', (t) => {
  const { scan } = fixture(t);
  const result = scan();
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Secret scan passed/);
});

test('deleted blobs, commit messages, annotated tags and untracked files are scanned without exposing values', (t) => {
  const { root, git, scan } = fixture(t);
  // Generated fixtures are syntactically realistic but are never issued keys.
  const values = Array.from({ length: 4 }, () => `ghp_${randomBytes(18).toString('hex')}`);
  fs.writeFileSync(path.join(root, 'deleted.txt'), `credential = ${values[0]}\n`);
  git('add', 'deleted.txt');
  git('commit', '-qm', 'Historical fixture');
  git('rm', '-q', 'deleted.txt');
  git('commit', '-qm', `Message fixture ${values[1]}`);
  git('tag', '-a', 'fixture', '-m', `Annotated fixture ${values[2]}`);
  fs.writeFileSync(path.join(root, 'untracked.txt'), `credential = ${values[3]} # gitleaks:allow\n`);
  const result = scan();
  assert.equal(result.status, 1);
  const output = result.stdout + result.stderr;
  for (const kind of ['blob ', 'commit ', 'tag ', 'worktree ']) assert.ok(output.includes(kind));
  assert.ok(values.every((value) => !output.includes(value)), 'Secret values must never appear in output');
});

test('an external symlink is scanned as a link without reading its target', (t) => {
  const { root, scan } = fixture(t);
  const outside = fs.mkdtempSync(path.join(os.tmpdir(), 'wakenote-private-fixture-'));
  t.after(() => fs.rmSync(outside, { recursive: true, force: true }));
  const source = path.join(outside, 'private.txt');
  fs.writeFileSync(source, `credential = ghp_${randomBytes(18).toString('hex')}\n`);
  fs.symlinkSync(source, path.join(root, 'link.txt'));
  const result = scan();
  assert.equal(result.status, 0, result.stderr);
});

test('a shallow clone fails instead of reporting complete history coverage', (t) => {
  const { root } = fixture(t);
  const destination = fs.mkdtempSync(path.join(os.tmpdir(), 'wakenote-shallow-test-'));
  t.after(() => fs.rmSync(destination, { recursive: true, force: true }));
  const clone = spawnSync('git', ['clone', '-q', '--depth=1', `file://${root}`, destination]);
  assert.equal(clone.status, 0);
  const result = spawnSync(process.execPath, [scanner, destination], { encoding: 'utf8' });
  assert.equal(result.status, 2);
  assert.doesNotMatch(result.stdout, /Secret scan passed/);
});
