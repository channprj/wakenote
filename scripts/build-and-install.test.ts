import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const script = readFileSync(new URL("./build-and-install.sh", import.meta.url), "utf8");

describe("build-and-install script", () => {
  it("documents and parses --open as a launch-after-install option", () => {
    expect(script).toContain("scripts/build-and-install.sh --open");
    expect(script).toMatch(/\s--launch\|--open\)\s+LAUNCH_AFTER_INSTALL=1;/);
  });
});
