import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import {
  requestTranslation,
  type TranslationPreferences,
} from "@/lib/text-translation";
import { TRANSLATION_LANGUAGES } from "./settings/TranslationControls";

export function TranslatedText({
  text,
  preferences,
}: {
  text: string;
  preferences: TranslationPreferences;
}) {
  const { enabled, language, model, configured } = preferences;
  const [attempt, setAttempt] = useState(0);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [state, setState] = useState<{
    key: string;
    text?: string;
    error?: string;
    loading: boolean;
  }>({ key: "", loading: false });
  const key = JSON.stringify([text, language, model]);
  const canTranslate = enabled && language !== null && language !== "auto";
  const current =
    state.key === key && canTranslate ? state : { key, loading: false };
  const languageName =
    TRANSLATION_LANGUAGES.find((item) => item.value === language)?.label ??
    language;

  useEffect(() => {
    if (!canTranslate || language === null || !configured || !text.trim())
      return;
    let disposed = false;
    setState({ key, loading: true });
    const request = requestTranslation(text, language, model);
    void request.promise.then(
      (result) => {
        if (!disposed) setState({ key, text: result.text, loading: false });
      },
      (error) => {
        if (!disposed)
          setState({
            key,
            error: error instanceof Error ? error.message : String(error),
            loading: false,
          });
      },
    );
    return () => {
      disposed = true;
      request.cancel();
    };
  }, [key, text, canTranslate, language, model, configured, attempt]);

  return (
    <div className="grid min-w-0 gap-1">
      <p className="whitespace-pre-wrap">{text}</p>
      {current.text ? (
        <div className="border-l-2 border-primary/30 pl-2">
          <p className="text-xs text-muted-foreground">
            {languageName} translation
          </p>
          <p className="whitespace-pre-wrap">{current.text}</p>
        </div>
      ) : null}
      {current.error ? (
        <p role="alert" className="text-xs text-destructive">
          {current.error}
        </p>
      ) : null}
      {current.loading || current.error || current.text ? (
        <div className="flex items-center gap-2">
          {current.loading ? (
            <span
              role="status"
              className="inline-flex items-center gap-1 text-xs text-muted-foreground"
            >
              <Spinner /> Translating…
            </span>
          ) : !current.text ? (
            <Button
              size="sm"
              variant="ghost"
              disabled={!configured || !text.trim()}
              title={
                configured
                  ? "Translate this text with OpenRouter"
                  : "Add an OpenRouter API key in Integrations"
              }
              onClick={() => setAttempt((value) => value + 1)}
            >
              Retry translation
            </Button>
          ) : (
            <Button
              size="sm"
              variant="ghost"
              onClick={async () => {
                try {
                  if (!navigator.clipboard?.writeText)
                    throw new Error("Clipboard access is unavailable");
                  await navigator.clipboard.writeText(current.text!);
                  setCopiedKey(key);
                  setState((latest) =>
                    latest.key === key
                      ? { ...latest, error: undefined }
                      : latest,
                  );
                } catch (error) {
                  setState((latest) =>
                    latest.key === key
                      ? {
                          ...latest,
                          error:
                            error instanceof Error
                              ? error.message
                              : String(error),
                          loading: false,
                        }
                      : latest,
                  );
                }
              }}
            >
              {copiedKey === key ? "Copied" : "Copy translation"}
            </Button>
          )}
        </div>
      ) : null}
    </div>
  );
}
