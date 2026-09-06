import { useEffect, useRef, useState } from "react";
import { ArrowUpRight, Check, Copy, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { loadRecentDictations } from "@/lib/tauri-client";
import { formatLocalTimestamp } from "@/lib/transcript-history";
import type { AppSnapshot, RecentTranscript } from "@/lib/types";
import { SettingsCard } from "./settings-controls";

export function RecentDictations({
  snapshot,
  onOpenHistory,
}: {
  snapshot: AppSnapshot;
  onOpenHistory?: (entry?: RecentTranscript) => void;
}) {
  const [entries, setEntries] = useState<RecentTranscript[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refresh, setRefresh] = useState(0);
  const [copied, setCopied] = useState<string | null>(null);
  const [copyError, setCopyError] = useState<string | null>(null);
  const copyRequest = useRef(0);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(null);
    void loadRecentDictations(10)
      .then((next) => {
        if (active) setEntries(next);
      })
      .catch((cause) => {
        if (active)
          setError(cause instanceof Error ? cause.message : String(cause));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [snapshot.settings.save_root, snapshot.recent_transcripts, refresh]);

  useEffect(() => {
    setEntries([]);
    setCopied(null);
    setCopyError(null);
    return () => {
      copyRequest.current += 1;
    };
  }, [snapshot.settings.save_root]);

  async function copyEntry(entry: RecentTranscript) {
    const request = ++copyRequest.current;
    setCopied(null);
    setCopyError(null);
    try {
      await navigator.clipboard.writeText(entry.text);
      if (request === copyRequest.current) setCopied(entry.transcript_path);
    } catch (cause) {
      if (request === copyRequest.current) {
        setCopyError(
          `Could not copy dictation: ${cause instanceof Error ? cause.message : String(cause)}`,
        );
      }
    }
  }

  return (
    <SettingsCard
      title="Recent Dictations"
      description="Your 10 latest saved dictation transcripts on this Mac. Expand a result to read or copy it. Hidden transcripts stay hidden."
    >
      <div className="flex flex-wrap items-center justify-between gap-2 pb-3">
        <Button
          size="sm"
          variant="outline"
          disabled={!onOpenHistory}
          onClick={() => onOpenHistory?.(entries[0])}
        >
          View all dictations <ArrowUpRight aria-hidden="true" />
        </Button>
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label="Refresh recent dictations"
          disabled={loading}
          onClick={() => setRefresh((value) => value + 1)}
        >
          <RefreshCw
            className={loading ? "animate-spin" : undefined}
            aria-hidden="true"
          />
        </Button>
      </div>
      {error ? (
        <p role="alert" className="py-3 text-sm text-destructive">
          Could not load recent dictations: {error}
        </p>
      ) : null}
      {copyError ? (
        <p role="alert" className="py-3 text-sm text-destructive">
          {copyError}
        </p>
      ) : null}
      {loading && entries.length === 0 ? (
        <p role="status" className="py-4 text-sm text-muted-foreground">
          Loading recent dictations…
        </p>
      ) : null}
      {!loading && !error && entries.length === 0 ? (
        <p className="py-4 text-sm text-muted-foreground">
          No recent dictations. Use either dictation hotkey to record your first
          one.
        </p>
      ) : null}
      {entries.length > 0 ? (
        <ol aria-label="Recent dictations" className="divide-y divide-border">
          {entries.map((entry) => {
            const timestamp = formatLocalTimestamp(entry.recorded_at);
            const isCopied = copied === entry.transcript_path;
            return (
              <li key={entry.transcript_path} className="py-3">
                <details className="group">
                  <summary className="cursor-pointer rounded-sm focus-visible:outline-2 focus-visible:outline-ring">
                    <time
                      dateTime={entry.recorded_at}
                      className="text-xs text-muted-foreground"
                    >
                      {timestamp}
                    </time>
                    <span className="mt-1 line-clamp-2 text-sm group-open:hidden">
                      {entry.text}
                    </span>
                  </summary>
                  <p className="mt-2 whitespace-pre-wrap break-words text-sm">
                    {entry.text}
                  </p>
                  {entry.device_name ? (
                    <p className="mt-2 text-xs text-muted-foreground">
                      Microphone: {entry.device_name}
                    </p>
                  ) : null}
                  <div className="mt-3 flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => void copyEntry(entry)}
                    >
                      {isCopied ? (
                        <Check aria-hidden="true" />
                      ) : (
                        <Copy aria-hidden="true" />
                      )}
                      {isCopied ? "Copied" : "Copy text"}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={!onOpenHistory}
                      onClick={() => onOpenHistory?.(entry)}
                    >
                      View in Transcripts <ArrowUpRight aria-hidden="true" />
                    </Button>
                  </div>
                </details>
              </li>
            );
          })}
        </ol>
      ) : null}
      <span role="status" className="sr-only">
        {copied ? "Dictation copied to clipboard" : ""}
      </span>
    </SettingsCard>
  );
}
