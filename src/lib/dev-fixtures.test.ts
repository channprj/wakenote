import { describe, expect, it } from "vitest";
import { devFixtures } from "./dev-fixtures";
import { isActiveLlmReportRun } from "./llm-report-runs";
import {
  listLlmReportHistory,
  listLlmReportRuns,
  loadLlmReportHistoryDetail,
  loadSnapshot,
  loadTranscriptDays,
  loadTranscriptsForDay,
  seedBrowserFixtures,
} from "./tauri-client";

// Fixed reference day so relative-date fixtures stay assertable.
const now = new Date("2026-07-27T21:00:00+09:00");

describe("dev fixtures", () => {
  it("derives transcript paths from each capture's own timestamp", () => {
    const { transcripts } = devFixtures(now);

    expect(transcripts.length).toBeGreaterThan(0);
    for (const transcript of transcripts) {
      const recorded = new Date(transcript.recorded_at);
      const folder = [
        recorded.getFullYear(),
        String(recorded.getMonth() + 1).padStart(2, "0"),
        String(recorded.getDate()).padStart(2, "0"),
      ].join("");
      const basename = [
        recorded.getHours(),
        recorded.getMinutes(),
        recorded.getSeconds(),
      ]
        .map((part) => String(part).padStart(2, "0"))
        .join("");

      expect(transcript.transcript_path).toBe(
        `~/Documents/WakeNote/${folder}/${basename}.txt`,
      );
      expect(transcript.audio_path).toBe(
        `~/Documents/WakeNote/${folder}/${basename}.m4a`,
      );
    }
  });

  it("spreads captures across several days so the day strip has content", () => {
    const { transcripts } = devFixtures(now);
    const days = new Set(
      transcripts.map((transcript) => transcript.recorded_at.slice(0, 10)),
    );

    expect(days.size).toBeGreaterThanOrEqual(4);
  });

  it("covers both capture sources", () => {
    const { transcripts } = devFixtures(now);
    const sources = new Set(transcripts.map((transcript) => transcript.source));

    expect(sources).toEqual(new Set(["microphone", "system"]));
  });

  it("supplies one report of each kind with renderable Markdown bodies", () => {
    const { reports } = devFixtures(now);
    const kinds = reports.map((report) => report.item.kind);

    expect(kinds).toContain("summary");
    expect(kinds).toContain("detailed_report");

    for (const report of reports) {
      // Exercises the pieces a Markdown renderer has to handle.
      expect(report.content).toMatch(/^# /m);
      expect(report.content).toMatch(/^## /m);
      expect(report.content).toContain("| --- |");
      expect(report.content).toMatch(/^- /m);
      expect(report.item.usage?.total_tokens).toBeGreaterThan(0);
    }
  });

  it("includes an in-flight run, completed runs, and a failed run", () => {
    const { reports, runs } = devFixtures(now);

    expect(runs.filter(isActiveLlmReportRun)).toHaveLength(1);
    expect(runs.filter((run) => run.status === "failed")).toHaveLength(1);

    const completedReportIds = runs
      .filter((run) => run.status === "completed")
      .map((run) => run.report_id);
    for (const report of reports) {
      expect(completedReportIds).toContain(report.item.report_id);
    }
  });

  it("keeps the in-flight run's progress trail attributed to that run", () => {
    const { runs } = devFixtures(now);
    const active = runs.find(isActiveLlmReportRun);

    expect(active?.progress.length).toBeGreaterThan(0);
    for (const event of active?.progress ?? []) {
      expect(event.run_id).toBe(active?.run_id);
    }
  });

  it("marks some models installed so model pickers are usable", () => {
    const { models, settings } = devFixtures(now);
    const installed = models.filter((model) => model.status === "installed");

    expect(installed.length).toBeGreaterThan(0);
    expect(installed.map((model) => model.id)).toContain(
      settings.selected_model,
    );
  });
});

describe("seedBrowserFixtures", () => {
  it("makes the browser mock serve the seeded notes, reports, and runs", async () => {
    const fixtures = devFixtures(now);
    seedBrowserFixtures(fixtures);

    const snapshot = await loadSnapshot();
    expect(snapshot.recent_transcripts).toHaveLength(
      fixtures.transcripts.length,
    );
    expect(snapshot.openrouter_key_configured).toBe(true);

    const days = await loadTranscriptDays();
    expect(days.length).toBeGreaterThanOrEqual(4);
    const busiest = days.reduce((left, right) =>
      right.count > left.count ? right : left,
    );
    const entries = await loadTranscriptsForDay(busiest.day);
    expect(entries).toHaveLength(busiest.count);

    const history = await listLlmReportHistory();
    expect(history.map((item) => item.report_id)).toEqual(
      fixtures.reports.map((report) => report.item.report_id),
    );

    const detail = await loadLlmReportHistoryDetail(
      fixtures.reports[0].item.report_id,
    );
    expect(detail.content).toBe(fixtures.reports[0].content);

    const runs = await listLlmReportRuns();
    expect(runs.filter(isActiveLlmReportRun)).toHaveLength(1);
  });
});
