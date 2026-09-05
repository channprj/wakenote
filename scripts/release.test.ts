import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { runRelease } from "./release.mjs";

const version = "0.260906.0";
const tag = `v${version}`;
const artifact = `WakeNote_${version}_aarch64.dmg`;
let root: string;
let commit: string;
let calls: string[][];
let actionsEnabled: boolean;
let remoteCommit: string;
let releases: string;
let failBuild: boolean;
let failPublish: boolean;
let changeSourceDuringBuild: boolean;
let duringBuild: () => void;

function git(...args: string[]) {
  const result = spawnSync("git", args, { cwd: root, encoding: "utf8" });
  if (result.status !== 0) throw new Error(result.stderr);
  return result.stdout.trim();
}

function write(relative: string, content: string) {
  const target = path.join(root, relative);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, content);
}

// Only native packaging and GitHub are replaced. Git state, version parsing,
// output selection, manifests and checksums exercise the production code.
function run(command: string, args: string[]) {
  calls.push([command, ...args]);
  if (command === "git") {
    if (args[0] === "ls-remote") {
      return `${git("rev-parse", `refs/tags/${tag}`)}\trefs/tags/${tag}\n${remoteCommit}\trefs/tags/${tag}^{}\n${remoteCommit}\trefs/heads/main`;
    }
    return git(...args);
  }
  if (command === process.execPath) {
    if (failBuild) throw new Error("native build failed");
    duringBuild();
    if (changeSourceDuringBuild) write("new-source.rs", "changed while building");
    write(`src-tauri/target/release/bundle/dmg/${artifact}`, "fresh DMG");
    // A more recent wrong-version file must never become a release asset.
    write("src-tauri/target/release/bundle/dmg/WakeNote_9.991231.0_aarch64.dmg", "stale DMG");
    return "";
  }
  if (command === "plutil") return version;
  if (command === "lipo") return "arm64";
  if (command === "codesign" || command === "hdiutil") return "";
  if (command === "gh" && args[0] === "api") {
    if (args.some((arg) => arg.endsWith("actions/permissions"))) {
      return JSON.stringify({ enabled: actionsEnabled });
    }
    return releases;
  }
  if (command === "gh" && args[0] === "release" && args[1] === "create") {
    if (failPublish) throw new Error("GitHub upload failed");
    return `https://github.com/channprj/wakenote/releases/tag/${tag}`;
  }
  throw new Error(`Unexpected command: ${command} ${args.join(" ")}`);
}

function release(mode: "build" | "publish", options = {}) {
  return runRelease({ root, mode, platform: "darwin", arch: "arm64", run, ...options });
}

function outputDir() {
  return path.join(root, "release", tag, "aarch64");
}

beforeEach(() => {
  root = fs.mkdtempSync(path.join(os.tmpdir(), "wakenote-release-test-"));
  write("VERSION", `${version}\n`);
  write("package.json", JSON.stringify({ name: "wakenote", version }));
  write("src-tauri/tauri.conf.json", JSON.stringify({ productName: "WakeNote", version }));
  write("src-tauri/Cargo.toml", `[package]\nname = "wakenote"\nversion = "${version}"\n`);
  write("src-tauri/Cargo.lock", `version = 4\n\n[[package]]\nname = "other"\nversion = "1.0.0"\n\n[[package]]\nname = "wakenote"\nversion = "${version}"\n`);
  write(".gitignore", "release/\nsrc-tauri/target/\n");
  git("init", "-b", "main");
  git("config", "user.name", "Release test");
  git("config", "user.email", "release@example.test");
  git("add", "VERSION", "package.json", "src-tauri", ".gitignore");
  git("-c", "commit.gpgsign=false", "commit", "-m", "fixture");
  git("-c", "tag.gpgsign=false", "tag", "-a", tag, "-m", tag);
  git("remote", "add", "origin", "https://github.com/channprj/wakenote.git");
  git("config", "branch.main.remote", "origin");
  git("config", "branch.main.merge", "refs/heads/main");
  commit = git("rev-parse", "HEAD");
  remoteCommit = commit;
  actionsEnabled = false;
  releases = "v0.260823.0\n";
  failBuild = false;
  failPublish = false;
  changeSourceDuringBuild = false;
  duringBuild = () => {};
  calls = [];
});

afterEach(() => {
  vi.restoreAllMocks();
  fs.rmSync(root, { recursive: true, force: true });
});

describe("local release build", () => {
  it("performs a build dry-run without invoking the native builder or creating files", async () => {
    await release("build", { dryRun: true });
    expect(calls.every(([command]) => command === "git")).toBe(true);
    expect(fs.existsSync(path.join(root, "release"))).toBe(false);
  });

  it("packages the exact current-version artifact with a checksum and source commit, without GitHub calls", async () => {
    await release("build");
    expect(fs.readFileSync(path.join(outputDir(), artifact), "utf8")).toBe("fresh DMG");
    const hash = createHash("sha256").update("fresh DMG").digest("hex");
    expect(fs.readFileSync(path.join(outputDir(), "SHA256SUMS.txt"), "utf8")).toBe(`${hash}  ${artifact}\n`);
    expect(JSON.parse(fs.readFileSync(path.join(outputDir(), "release.json"), "utf8"))).toMatchObject({
      version, commit, arch: "aarch64", artifact, sha256: hash,
    });
    expect(calls.some(([command]) => command === "gh")).toBe(false);
  });

  it.each(["package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"])("rejects inconsistent %s before invoking the native builder", async (file) => {
    write(file, fs.readFileSync(path.join(root, file), "utf8").replace(version, "0.260905.0"));
    await expect(release("build")).rejects.toThrow(/version/i);
    expect(calls.some(([command]) => command === process.execPath)).toBe(false);
  });

  it("rejects uncommitted source before building", async () => {
    write("new-source.rs", "uncommitted");
    await expect(release("build")).rejects.toThrow(/clean/i);
  });

  it("leaves no releasable output after a failed native build", async () => {
    failBuild = true;
    await expect(release("build")).rejects.toThrow("native build failed");
    expect(fs.existsSync(path.join(outputDir(), "release.json"))).toBe(false);
  });

  it("rejects source edits made while the native builder was running", async () => {
    changeSourceDuringBuild = true;
    await expect(release("build")).rejects.toThrow(/clean/i);
    expect(fs.existsSync(path.join(outputDir(), "release.json"))).toBe(false);
  });

  it("rejects unsupported hosts before building", async () => {
    await expect(release("build", { platform: "linux" })).rejects.toThrow(/Mac/);
    expect(calls).toEqual([]);
  });

  it("preserves existing release output on a repeated build", async () => {
    await release("build");
    calls = [];
    await expect(release("build")).rejects.toThrow(/already exists/i);
    expect(calls.some(([command]) => command === process.execPath)).toBe(false);
    expect(fs.readFileSync(path.join(outputDir(), artifact), "utf8")).toBe("fresh DMG");
  });
});

describe("manual release publication", () => {
  it("checks a release without publishing during dry-run", async () => {
    await release("build");
    const log = vi.spyOn(console, "log").mockImplementation(() => {});
    calls = [];
    await release("publish", { dryRun: true });
    expect(log.mock.calls.flat().join("\n")).toContain("Would reuse verified local release");
    expect(calls.some(([command]) => command === process.execPath)).toBe(false);
    expect(calls.some(([command, action]) => command === "gh" && action === "api")).toBe(true);
    expect(calls.some(([command, action]) => command === "gh" && action === "release")).toBe(false);
  });

  it("uploads only verified files to the existing tag on explicit publication", async () => {
    await release("build");
    calls = [];
    await release("publish", { draft: true });
    const create = calls.find(([command, action, verb]) => command === "gh" && action === "release" && verb === "create");
    expect(create).toEqual(expect.arrayContaining([
      tag, "--repo", "channprj/wakenote", "--verify-tag", "--draft",
      path.join(outputDir(), artifact), path.join(outputDir(), "SHA256SUMS.txt"), path.join(outputDir(), "release.json"),
    ]));
    expect(create).not.toContain("--clobber");
    expect(calls.some(([command]) => command === process.execPath)).toBe(false);
    expect(calls.some(([command, action]) => command === "git" && action === "push")).toBe(false);
  });

  it.each([false, true])("builds missing artifacts before publishing (draft: %s)", async (draft) => {
    await release("publish", { draft });
    const buildIndex = calls.findIndex(([command]) => command === process.execPath);
    const createIndex = calls.findIndex(([command, action]) => command === "gh" && action === "release");
    expect(buildIndex).toBeGreaterThan(-1);
    expect(createIndex).toBeGreaterThan(buildIndex);
    expect(calls.filter(([command]) => command === process.execPath)).toHaveLength(1);
    expect(calls[createIndex].includes("--draft")).toBe(draft);
    expect(calls.some(([command, action]) => command === "git" && action === "push")).toBe(false);
    expect(JSON.parse(fs.readFileSync(path.join(outputDir(), "release.json"), "utf8"))).toMatchObject({ version, commit });
  });

  it.each(["artifact", "checksum", "commit", "version", "arch", "artifact name", "missing manifest", "missing DMG", "missing checksum", "invalid JSON", "null manifest"])("rebuilds and preserves an unusable cache: %s", async (condition) => {
    await release("build");
    const manifestPath = path.join(outputDir(), "release.json");
    if (condition === "artifact") fs.appendFileSync(path.join(outputDir(), artifact), "corrupt");
    if (condition === "checksum") fs.writeFileSync(path.join(outputDir(), "SHA256SUMS.txt"), "invalid");
    if (["commit", "version", "arch", "artifact name"].includes(condition)) {
      const key = condition === "artifact name" ? "artifact" : condition;
      const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
      fs.writeFileSync(manifestPath, JSON.stringify({ ...manifest, [key]: "stale" }));
    }
    if (condition === "missing manifest") fs.unlinkSync(manifestPath);
    if (condition === "missing DMG") fs.unlinkSync(path.join(outputDir(), artifact));
    if (condition === "missing checksum") fs.unlinkSync(path.join(outputDir(), "SHA256SUMS.txt"));
    if (condition === "invalid JSON") fs.writeFileSync(manifestPath, "{");
    if (condition === "null manifest") fs.writeFileSync(manifestPath, "null");
    fs.writeFileSync(path.join(outputDir(), "previous-output.txt"), "keep this");
    calls = [];
    await release("publish");
    expect(calls.filter(([command]) => command === process.execPath)).toHaveLength(1);
    expect(calls.some(([command, action]) => command === "gh" && action === "release")).toBe(true);
    expect(fs.readFileSync(path.join(outputDir(), artifact), "utf8")).toBe("fresh DMG");
    const parent = path.dirname(outputDir());
    const backups = fs.readdirSync(parent).filter((name) => name.startsWith(".previous-"));
    expect(backups).toHaveLength(1);
    expect(fs.readFileSync(path.join(parent, backups[0], "aarch64", "previous-output.txt"), "utf8")).toBe("keep this");
  });

  it.each(["missing", "stale", "corrupt"])("reports a needed build during dry-run without changing %s artifacts", async (condition) => {
    if (condition !== "missing") {
      await release("build");
      const manifestPath = path.join(outputDir(), "release.json");
      const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
      if (condition === "stale") fs.writeFileSync(manifestPath, JSON.stringify({ ...manifest, commit: "stale" }));
      if (condition === "corrupt") fs.appendFileSync(path.join(outputDir(), artifact), "corrupt");
    }
    const snapshot = () => fs.existsSync(outputDir())
      ? fs.readdirSync(outputDir()).map((name) => [name, fs.readFileSync(path.join(outputDir(), name), "utf8")])
      : null;
    const before = snapshot();
    const log = vi.spyOn(console, "log").mockImplementation(() => {});
    calls = [];
    await release("publish", { dryRun: true });
    expect(log.mock.calls.flat().join("\n")).toContain(`Would build ${artifact}`);
    expect(snapshot()).toEqual(before);
    expect(calls.some(([command]) => command === process.execPath)).toBe(false);
    expect(calls.some(([command, action]) => command === "gh" && action === "release")).toBe(false);
    if (condition === "missing") expect(fs.existsSync(path.join(root, "release"))).toBe(false);
    else expect(fs.readdirSync(path.dirname(outputDir()))).toEqual(["aarch64"]);
  });

  it.each(["missing", "stale"])("does not publish or discard %s output after a failed automatic build", async (condition) => {
    if (condition === "stale") {
      await release("build");
      fs.writeFileSync(path.join(outputDir(), "release.json"), "stale manifest");
    }
    calls = [];
    failBuild = true;
    await expect(release("publish")).rejects.toThrow("native build failed");
    expect(calls.some(([command, action]) => command === "gh" && action === "release")).toBe(false);
    if (condition === "stale") {
      expect(fs.readFileSync(path.join(outputDir(), "release.json"), "utf8")).toBe("stale manifest");
      expect(fs.readFileSync(path.join(outputDir(), artifact), "utf8")).toBe("fresh DMG");
    } else {
      expect(fs.existsSync(outputDir())).toBe(false);
    }
  });

  it.each(["source", "remote", "actions", "existing"])("rechecks changed %s state after an automatic build", async (condition) => {
    duringBuild = () => {
      if (condition === "source") write("new-source.rs", "changed while building");
      if (condition === "remote") remoteCommit = "b".repeat(40);
      if (condition === "actions") actionsEnabled = true;
      if (condition === "existing") releases = `${tag}\n`;
    };
    await expect(release("publish")).rejects.toThrow();
    expect(calls.some(([command]) => command === process.execPath)).toBe(true);
    expect(calls.some(([command, action]) => command === "gh" && action === "release")).toBe(false);
  });

  it("restores the old output if installing the rebuilt directory fails", async () => {
    await release("build");
    fs.writeFileSync(path.join(outputDir(), "release.json"), "stale manifest");
    const rename = fs.renameSync;
    vi.spyOn(fs, "renameSync").mockImplementation((from, to) => {
      if (path.basename(String(from)).startsWith(".building-")) throw new Error("install failed");
      rename(from, to);
    });
    calls = [];
    await expect(release("publish")).rejects.toThrow("install failed");
    expect(fs.readFileSync(path.join(outputDir(), "release.json"), "utf8")).toBe("stale manifest");
    expect(fs.readFileSync(path.join(outputDir(), artifact), "utf8")).toBe("fresh DMG");
    expect(fs.readdirSync(path.dirname(outputDir())).some((name) => name.startsWith(".building-"))).toBe(false);
    expect(calls.some(([command, action]) => command === "gh" && action === "release")).toBe(false);
  });

  it("reuses the completed build when retrying a failed upload with no existing release", async () => {
    failPublish = true;
    await expect(release("publish")).rejects.toThrow("GitHub upload failed");
    expect(fs.existsSync(path.join(outputDir(), "release.json"))).toBe(true);
    failPublish = false;
    calls = [];
    await release("publish");
    expect(calls.some(([command]) => command === process.execPath)).toBe(false);
    expect(calls.some(([command, action]) => command === "gh" && action === "release")).toBe(true);
  });

  it.each(["remote", "actions", "existing", "tag"])("blocks publication for invalid %s before automatically building", async (condition) => {
    if (condition === "remote") remoteCommit = "b".repeat(40);
    if (condition === "actions") actionsEnabled = true;
    if (condition === "existing") releases = `${tag}\n`;
    if (condition === "tag") git("tag", "-d", tag);
    calls = [];
    await expect(release("publish")).rejects.toThrow();
    expect(calls.some(([command]) => command === process.execPath)).toBe(false);
    expect(calls.some(([command, action]) => command === "gh" && action === "release")).toBe(false);
  });
});
