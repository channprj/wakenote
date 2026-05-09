import { describe, expect, it } from "vitest";
import config from "./vite.config";

describe("vite config", () => {
  it("builds assets with relative URLs for the Tauri static bundle", () => {
    expect(config).toMatchObject({ base: "./" });
  });
});
