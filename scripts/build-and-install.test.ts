import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const wrapperScript = readFileSync(new URL("./build-and-install.sh", import.meta.url), "utf8");
const buildScript = readFileSync(new URL("./build.mjs", import.meta.url), "utf8");
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

describe("package.json scripts", () => {
  it("exposes pnpm build install [open] convenience aliases", () => {
    const scripts = packageJson.scripts ?? {};
    expect(scripts.build).toBe("node scripts/build.mjs");
    expect(scripts["build:debug"]).toBe("pnpm build debug");
    expect(scripts["build:install"]).toBe("pnpm build install");
    expect(scripts["build:install:open"]).toBe("pnpm build install open");
    expect(scripts["build:install:debug"]).toBe("pnpm build debug install");
    expect(scripts["build:install:debug:open"]).toBe("pnpm build debug install open");
  });
});
