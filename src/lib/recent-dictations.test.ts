import { describe, expect, it } from "vitest";
import { devFixtures } from "./dev-fixtures";
import {
  loadRecentDictations,
  seedBrowserFixtures,
  setListVisibility,
} from "./tauri-client";

describe("recent dictations browser history", () => {
  it("filters and sorts before applying a limit, and excludes hidden records", async () => {
    const fixture = devFixtures(new Date("2026-09-07T12:00:00+09:00"));
    const dictations = fixture.transcripts.filter(
      (entry) => entry.source_label === "dictation",
    );
    expect(dictations).toHaveLength(2);
    fixture.transcripts = [...fixture.transcripts].reverse();
    seedBrowserFixtures(fixture);
    const entries = await loadRecentDictations(1);
    expect(entries).toHaveLength(1);
    expect(entries[0].recorded_at).toBe(dictations[0].recorded_at);
    await setListVisibility({
      targets: [{ kind: "transcript", id: entries[0].transcript_path }],
      hidden: true,
    });
    const visible = await loadRecentDictations(1);
    expect(visible).toEqual([dictations[1]]);
    expect(await loadRecentDictations(0)).toEqual([]);
  });
});
