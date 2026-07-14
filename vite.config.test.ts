import { describe, expect, it } from "vitest";
import { configDefaults } from "vitest/config";
import config from "./vite.config";

describe("vite config", () => {
  it("builds assets with relative URLs for the Tauri static bundle", () => {
    expect(config).toMatchObject({ base: "./" });
  });

  it("ignores nested worktrees without dropping Vitest default excludes", () => {
    expect(config.test?.exclude).toEqual(
      expect.arrayContaining([...configDefaults.exclude, ".worktrees/**"]),
    );
  });
});
