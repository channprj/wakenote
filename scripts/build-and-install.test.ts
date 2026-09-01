import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const wrapperScript = readFileSync(new URL("./build-and-install.sh", import.meta.url), "utf8");
const buildScript = readFileSync(new URL("./build.mjs", import.meta.url), "utf8");
const nativeBuildPreflightUrl = new URL("./native-build-preflight.mjs", import.meta.url);
const packageJson = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
);

describe("build-and-install wrapper", () => {
  it("documents --open and delegates to build.mjs", () => {
    expect(wrapperScript).toContain("scripts/build-and-install.sh --open");
    expect(wrapperScript).toContain("exec node scripts/build.mjs install");
  });
});

describe("scripts/build.mjs CLI", () => {
  it("checks native build tools before starting a Tauri build", () => {
    expect(buildScript).toMatch(
      /function buildTauri\(mode, bundle, env = process\.env\) \{\s+ensureNativeBuildTools\(\{ env \}\);/,
    );
  });

  it("refreshes dependencies from the frozen lockfile before Tauri builds", () => {
    expect(buildScript).toMatch(
      /function ensureDependencies\(\)[\s\S]*run\('pnpm', \['install', '--frozen-lockfile'\]\);/,
    );
    expect(buildScript).not.toContain(
      "if (!fs.existsSync(path.join(projectRoot, 'node_modules')))",
    );
  });

  it("cleans Cargo metadata that was created in another checkout", () => {
    expect(buildScript).toContain("function cleanRelocatedCargoArtifacts(mode)");
    expect(buildScript).toContain("'root-output'");
    expect(buildScript).toContain("path.relative(cargoTargetDir, cachedRoot)");
    expect(buildScript).toMatch(
      /run\('cargo', \['clean', '--manifest-path', cargoManifest, '--profile', cargoProfile\]\)/,
    );
    expect(buildScript).toMatch(
      /cleanRelocatedCargoArtifacts\(mode\);[\s\S]*pnpm[\s\S]*tauri/,
    );
  });

  it("treats open, --open, and --launch as launch flags that imply install", () => {
    expect(buildScript).toMatch(/arg === 'open' \|\| arg === '--open' \|\| arg === '--launch'/);
  });

  it("documents the pnpm build install [open] usage", () => {
    expect(buildScript).toContain("pnpm build install [open]");
  });

  it("targets the WakeNote.app bundle under src-tauri/target", () => {
    expect(buildScript).toContain("const APP_BUNDLE_NAME = 'WakeNote.app'");
    expect(buildScript).toContain("path.join(\n    tauriDir,\n    'target',");
  });

  it("quits a running WakeNote before installing", () => {
    expect(buildScript).toContain("quitRunningWakeNote()");
    expect(buildScript).toMatch(/PROCESS_NAMES = \['wakenote', 'sagwan'\]/);
  });
});

describe("native build preflight", () => {
  it("reports an actionable CMake installation error", () => {
    const result = spawnSync(process.execPath, [fileURLToPath(nativeBuildPreflightUrl)], {
      encoding: "utf8",
      env: {
        ...process.env,
        CMAKE: "/definitely/missing/wakenote-cmake",
      },
    });

    expect(result.status).toBe(1);
    expect(result.stderr).toContain(
      "CMake is required to compile the bundled Whisper runtime",
    );
    if (process.platform === "darwin") {
      expect(result.stderr).toContain("brew install cmake");
    }
    expect(result.stderr).toContain("cmake --version");
  });
});

describe("package.json scripts", () => {
  it("exposes pnpm build install [open] convenience aliases", () => {
    const scripts = packageJson.scripts ?? {};
    expect(scripts.build).toBe("node scripts/build.mjs");
    expect(scripts["native:preflight"]).toBe("node scripts/native-build-preflight.mjs");
    expect(scripts["build:debug"]).toBe("pnpm build debug");
    expect(scripts["build:install"]).toBe("pnpm build install");
    expect(scripts["build:install:open"]).toBe("pnpm build install open");
    expect(scripts["build:install:debug"]).toBe("pnpm build debug install");
    expect(scripts["build:install:debug:open"]).toBe("pnpm build debug install open");
  });
});
