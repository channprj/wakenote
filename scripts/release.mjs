#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function execute(root, command, args, inherit = false) {
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    stdio: inherit ? 'inherit' : 'pipe',
  });
  if (result.error || result.status !== 0) {
    throw new Error(result.error?.message || result.stderr?.trim() || `${command} failed (${result.status})`);
  }
  return result.stdout?.trim() ?? '';
}

function readVersion(root) {
  const read = (name) => fs.readFileSync(path.join(root, name), 'utf8');
  const version = read('VERSION').trim();
  if (!/^(0|[1-9]\d*)\.\d{6}\.(0|[1-9]\d*)$/.test(version)) {
    throw new Error('VERSION must use Headatever head.yymmdd.patch format.');
  }
  const cargoPackage = read('src-tauri/Cargo.toml').split(/^\[package\]\s*$/m)[1]?.split(/^\[/m)[0];
  const lockPackage = read('src-tauri/Cargo.lock').split('[[package]]').find((block) => /^name = "wakenote"$/m.test(block));
  const versions = {
    'package.json': JSON.parse(read('package.json')).version,
    'src-tauri/tauri.conf.json': JSON.parse(read('src-tauri/tauri.conf.json')).version,
    'src-tauri/Cargo.toml': cargoPackage?.match(/^version = "([^"]+)"$/m)?.[1],
    'src-tauri/Cargo.lock': lockPackage?.match(/^version = "([^"]+)"$/m)?.[1],
  };
  for (const [name, actual] of Object.entries(versions)) {
    if (actual !== version) throw new Error(`Version mismatch in ${name}: expected ${version}, found ${actual}`);
  }
  return version;
}

function sourceCommit(run) {
  if (run('git', ['status', '--porcelain=v1', '--untracked-files=all'])) {
    throw new Error('A clean worktree is required. Commit the intended source and version changes first.');
  }
  return run('git', ['rev-parse', 'HEAD']);
}

function githubRepo(run) {
  const origin = run('git', ['remote', 'get-url', 'origin']);
  const match = origin.match(/^(?:https:\/\/github\.com\/|git@github\.com:|ssh:\/\/git@github\.com\/)([\w.-]+\/[\w.-]+?)(?:\.git)?$/);
  if (!match) throw new Error('origin must identify a GitHub repository using HTTPS or SSH.');
  return match[1];
}

async function sha256(file) {
  const hash = createHash('sha256');
  for await (const chunk of fs.createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}

/** Build locally or publish, building current artifacts as needed. Never push Git refs. */
export async function runRelease({
  root = projectRoot,
  mode,
  dryRun = false,
  draft = false,
  platform = process.platform,
  arch = process.arch,
  run = (command, args, inherit) => execute(root, command, args, inherit),
}) {
  if (!['build', 'publish'].includes(mode)) throw new Error('Use release:build or release:publish.');
  if (platform !== 'darwin' || !['arm64', 'x64'].includes(arch)) {
    throw new Error('Release commands require an Apple Silicon or Intel Mac; builds target the current Mac only.');
  }
  const version = readVersion(root);
  const commit = sourceCommit(run);
  const tag = `v${version}`;
  const architecture = arch === 'arm64' ? 'aarch64' : 'x64';
  const artifact = `WakeNote_${version}_${architecture}.dmg`;
  const output = path.join(root, 'release', tag, architecture);
  const dmg = path.join(output, artifact);
  const checksums = path.join(output, 'SHA256SUMS.txt');
  const manifestPath = path.join(output, 'release.json');

  async function buildArtifacts(replaceExisting = false) {
    if (fs.existsSync(output) && !replaceExisting) {
      throw new Error(`Release output already exists: ${output}. Keep it for publication, or move this directory aside before rebuilding.`);
    }
    if (dryRun) {
      console.log(`Would build ${artifact} from ${commit} into ${output}`);
      return;
    }
    run(process.execPath, [path.join(root, 'scripts', 'build.mjs'), 'release', 'dmg'], true);
    const bundle = path.join(root, 'src-tauri', 'target', 'release', 'bundle');
    const app = path.join(bundle, 'macos', 'WakeNote.app');
    const builtDmg = path.join(bundle, 'dmg', artifact);
    run('codesign', ['--verify', '--deep', '--strict', app]);
    const bundledVersion = run('plutil', ['-extract', 'CFBundleShortVersionString', 'raw', '-o', '-', path.join(app, 'Contents', 'Info.plist')]);
    if (bundledVersion !== version) throw new Error('The built app version does not match VERSION.');
    if (run('lipo', ['-archs', path.join(app, 'Contents', 'MacOS', 'wakenote')]) !== arch) {
      throw new Error('The built app architecture does not match this Mac.');
    }
    run('hdiutil', ['verify', builtDmg]);
    if (readVersion(root) !== version || sourceCommit(run) !== commit) {
      throw new Error('Source changed during the build; commit the changes and rebuild.');
    }
    // Publish only a complete staged directory. Failed builds leave no manifest
    // that a later publish could mistake for a successful release build.
    const parent = path.dirname(output);
    fs.mkdirSync(parent, { recursive: true });
    const staging = fs.mkdtempSync(path.join(parent, '.building-'));
    try {
      const stagedDmg = path.join(staging, artifact);
      fs.copyFileSync(builtDmg, stagedDmg, fs.constants.COPYFILE_EXCL);
      const digest = await sha256(stagedDmg);
      fs.writeFileSync(path.join(staging, 'SHA256SUMS.txt'), `${digest}  ${artifact}\n`);
      fs.writeFileSync(path.join(staging, 'release.json'), `${JSON.stringify({
        version, commit, arch: architecture, artifact, sha256: digest,
        signing: 'ad-hoc', notarized: false,
      }, null, 2)}\n`);
      let backup;
      if (fs.existsSync(output)) {
        const previous = fs.mkdtempSync(path.join(parent, '.previous-'));
        backup = path.join(previous, architecture);
        fs.renameSync(output, backup);
        console.log(`Previous release output preserved: ${backup}`);
      }
      try {
        fs.renameSync(staging, output);
      } catch (error) {
        if (backup) fs.renameSync(backup, output);
        throw error;
      }
    } finally {
      fs.rmSync(staging, { recursive: true, force: true });
    }
    console.log(`Local release ready: ${output}`);
  }

  async function localBuildProblem() {
    try {
      const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
      if (!manifest || manifest.version !== version || manifest.commit !== commit || manifest.arch !== architecture || manifest.artifact !== artifact) {
        return 'Release manifest does not match the current source, version, or architecture.';
      }
      const digest = await sha256(dmg);
      if (digest !== manifest.sha256 || fs.readFileSync(checksums, 'utf8') !== `${digest}  ${artifact}\n`) {
        return 'Release checksum mismatch.';
      }
      return null;
    } catch (error) {
      if (error.code === 'ENOENT' || error instanceof SyntaxError) return 'Local release files are missing or invalid.';
      throw error;
    }
  }

  if (mode === 'build') {
    await buildArtifacts();
    return;
  }

  function verifyPublication() {
    const tagRef = `refs/tags/${tag}`;
    if (run('git', ['cat-file', '-t', tagRef]) !== 'tag' || run('git', ['rev-parse', `${tagRef}^{}`]) !== commit) {
      throw new Error(`${tag} must be an annotated tag pointing at HEAD.`);
    }
    const branch = run('git', ['symbolic-ref', '--short', 'HEAD']);
    if (run('git', ['config', `branch.${branch}.remote`]) !== 'origin') {
      throw new Error('The current branch must track origin before publication.');
    }
    const branchRef = run('git', ['config', `branch.${branch}.merge`]);
    const remoteRefs = new Map(run('git', ['ls-remote', 'origin', branchRef, tagRef, `${tagRef}^{}`])
      .split('\n').filter(Boolean).map((line) => {
        const [sha, ref] = line.split(/\s+/);
        return [ref, sha];
      }));
    if (remoteRefs.get(branchRef) !== commit || remoteRefs.get(`${tagRef}^{}`) !== commit || remoteRefs.get(tagRef) !== run('git', ['rev-parse', tagRef])) {
      throw new Error('Push the current branch and annotated tag to origin before publication; remote refs must match the local source.');
    }
    const repo = githubRepo(run);
    const permissions = JSON.parse(run('gh', ['api', `repos/${repo}/actions/permissions`]));
    if (permissions.enabled !== false) throw new Error('GitHub Actions must be disabled for this local-only release workflow.');
    const releases = run('gh', ['api', '--paginate', `repos/${repo}/releases`, '--jq', '.[].tag_name']);
    if (releases.split('\n').includes(tag)) throw new Error(`GitHub release ${tag} already exists. Inspect it before retrying; existing assets are never overwritten.`);
    return repo;
  }

  const repo = verifyPublication();
  const buildProblem = await localBuildProblem();
  if (dryRun) {
    console.log(buildProblem
      ? `${buildProblem}\nWould build ${artifact} from ${commit} into ${output}`
      : `Would reuse verified local release: ${output}`);
    console.log(`Verified publication prerequisites for ${tag} (${commit}) in ${repo}.\nWould ${draft ? 'create a draft with' : 'publish'}: ${artifact}, SHA256SUMS.txt, release.json\nNo build was run and no GitHub release was created.`);
    return;
  }
  if (buildProblem) {
    console.log(`${buildProblem}\nBuilding the current release before publication.`);
    await buildArtifacts(true);
    // Native builds can take time; refresh remote checks before uploading.
    if (verifyPublication() !== repo) throw new Error('The GitHub repository changed during the build.');
    const remainingProblem = await localBuildProblem();
    if (remainingProblem) throw new Error(remainingProblem);
  } else {
    console.log(`Reusing verified local release: ${output}`);
  }
  const args = [
    'release', 'create', tag, dmg, checksums, manifestPath,
    '--repo', repo, '--verify-tag', '--title', tag, '--generate-notes',
    '--notes', `Locally built macOS ${architecture} DMG. The app is ad-hoc signed and is not notarized. See USAGE.md for installation instructions.`,
    ...(draft ? ['--draft'] : []),
  ];
  // Hashing and network checks can take time; catch edits before the write.
  if (readVersion(root) !== version || sourceCommit(run) !== commit) throw new Error('Source changed before publication.');
  console.log(run('gh', args));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2).filter((arg) => arg !== '--');
  if (args.includes('--help') || args.includes('-h')) {
    console.log(`Usage:
  pnpm release:build [--dry-run]
  pnpm release:publish [--dry-run] [--draft]

Build a DMG for this Mac into release/v<version>/<architecture>/.
Publishing reuses artifacts matching the current commit, version, architecture
and checksums, or builds them first. Replaced local output is preserved.
Commit synchronized versions before building. Push the branch and annotated
version tag before publishing. Publishing requires gh authentication with
access to Actions settings and disabled Actions.
--dry-run validates and reports whether a build is needed, without building
or creating a GitHub release.
--draft uploads a draft release for review instead of publishing immediately.`);
  } else {
    const [mode, ...flags] = args;
    if (flags.some((flag) => !['--dry-run', '--draft'].includes(flag)) || (mode === 'build' && flags.includes('--draft'))) {
      console.error('Unknown release option. Use --help for usage.');
      process.exitCode = 2;
    } else {
      try {
        await runRelease({ mode, dryRun: flags.includes('--dry-run'), draft: flags.includes('--draft') });
      } catch (error) {
        console.error(`error: ${error.message}`);
        process.exitCode = 1;
      }
    }
  }
}
