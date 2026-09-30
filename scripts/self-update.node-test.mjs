import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, existsSync, rmSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { spawnSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { test } from 'node:test';

// Exercise the shipped installer with real filesystem renames and plutil.
// Only codesign/open are stubbed; swap failures can be injected at the mv boundary.
const script = readFileSync(new URL('../src-tauri/scripts/self-update.sh', import.meta.url), 'utf8');
function fixture(t, options = {}) {
  const base = realpathSync(mkdtempSync(path.join(tmpdir(), 'wakenote-installer-')));
  t.after(() => rmSync(base, { recursive: true, force: true }));
  const parent = path.join(base, 'Applications with spaces $ and ;');
  const root = path.join(parent, '.wakenote-update-test');
  const dest = path.join(parent, 'WakeNote.app');
  const bin = path.join(base, 'bin');
  mkdirSync(root, { recursive: true, mode: 0o700 });
  mkdirSync(bin);
  const bundle = (target, version, marker) => {
    mkdirSync(path.join(target, 'Contents'), { recursive: true });
    writeFileSync(path.join(target, 'Contents/Info.plist'), `<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.chann.wakenote</string><key>CFBundleShortVersionString</key><string>${version}</string></dict></plist>`);
    writeFileSync(path.join(target, 'marker'), marker);
  };
  bundle(dest, '0.260930.0', 'old');
  bundle(path.join(root, 'WakeNote.app'), options.wrongVersion ? '0.260929.0' : '0.261001.0', 'new');
  const stub = (name, source) => writeFileSync(path.join(bin, name), `#!/bin/bash\n${source}\n`, { mode: 0o755 });
  stub('codesign', options.badSignature ? 'exit 1' : 'exit 0');
  stub('osascript', 'exit 0');
  stub('open', `printf '%s\\n' "$1" >> "$OPEN_LOG"\n${options.openFailure ? 'exit 1' : 'exit 0'}`);
  stub('mv', `if [ "${options.swapFailure ? 'yes' : 'no'}" = yes ] && [ "$1" = "$UPDATE_ROOT/WakeNote.app" ]; then exit 1; fi\nexec /bin/mv "$@"`);
  writeFileSync(path.join(root, 'install.sh'), script);
  const env = { ...process.env, PATH: `${bin}:/usr/bin:/bin:/usr/sbin:/sbin`, OPEN_LOG: path.join(base, 'open.log'), UPDATE_ROOT: root };
  // A joined child PID is dead and cannot accidentally be our active process.
  const deadPid = spawnSync('/bin/bash', ['-c', 'echo $$'], { encoding: 'utf8' }).stdout.trim();
  const args = [path.join(root, 'install.sh'), deadPid, dest, '0.261001.0'];
  return { base, root, dest, env, args, marker: () => readFileSync(path.join(dest, 'marker'), 'utf8') };
}

test('verified update swaps the app and opens it, including paths with shell characters', (t) => {
  const f = fixture(t);
  const result = spawnSync('/bin/bash', f.args, { env: f.env });
  assert.equal(result.status, 0, result.stderr.toString());
  assert.equal(f.marker(), 'new');
  assert.equal(existsSync(f.root), false);
  assert.equal(readFileSync(f.env.OPEN_LOG, 'utf8').trim(), f.dest);
});

for (const scenario of ['wrongVersion', 'badSignature', 'swapFailure', 'openFailure']) {
  test(`${scenario} preserves or restores the old app`, (t) => {
    const f = fixture(t, { [scenario]: true });
    const result = spawnSync('/bin/bash', f.args, { env: f.env });
    assert.equal(result.status, 1);
    assert.equal(f.marker(), 'old');
    assert.equal(existsSync(f.root), false);
  });
}

test('a still-running app is never replaced, and interrupting the installer keeps it intact', async (t) => {
  const f = fixture(t);
  f.args[1] = String(process.pid);
  const child = spawn('/bin/bash', f.args, { env: f.env, stdio: 'ignore' });
  const completion = once(child, 'exit');
  await new Promise((resolve) => setTimeout(resolve, 250));
  assert.equal(f.marker(), 'old');
  child.kill('SIGTERM');
  const [code] = await completion;
  assert.equal(code, 1);
  assert.equal(f.marker(), 'old');
});
