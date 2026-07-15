import { describe, expect, it } from "vitest";
// @ts-ignore Vitest executes this source assertion in Node.
import { readFileSync } from "node:fs";

const config = JSON.parse(
  readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
);

describe("main window responsive boundary", () => {
  it("allows resizing down to the supported 760x640 compact viewport", () => {
    const mainWindow = config.app.windows[0];

    expect(mainWindow.minWidth).toBe(760);
    expect(mainWindow.minHeight).toBe(640);
    expect(mainWindow.resizable).toBe(true);
  });
});
