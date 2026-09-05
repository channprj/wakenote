import {
  cancelTextTransform,
  transformText,
  type TextTransformResult,
} from "./tauri-client";
import type { TranscriptionLanguage } from "./types";

export interface TranslationPreferences {
  enabled: boolean;
  language: TranscriptionLanguage;
  model: string;
  configured: boolean;
}

const cache = new Map<string, TextTransformResult>();
const waiting: Array<() => void> = [];
let active = 0;

function pump() {
  while (active < 2 && waiting.length) waiting.shift()?.();
}

export function requestTranslation(
  text: string,
  language: TranscriptionLanguage,
  model: string,
) {
  const key = JSON.stringify([text, language, model]);
  const cached = cache.get(key);
  if (cached) return { promise: Promise.resolve(cached), cancel() {} };
  const id = crypto.randomUUID();
  let cancelled = false;
  let started = false;
  let settled = false;
  let rejectRequest!: (error: Error) => void;
  const promise = new Promise<TextTransformResult>((resolve, reject) => {
    rejectRequest = reject;
    waiting.push(() => {
      if (cancelled) return;
      started = true;
      active += 1;
      void transformText(id, {
        kind: "translate",
        text,
        target_language: language,
      })
        .then(
          (result) => {
            if (!cancelled) {
              cache.set(key, result);
              if (cache.size > 128) cache.delete(cache.keys().next().value!);
              resolve(result);
            }
          },
          (error) => {
            if (!cancelled) reject(error);
          },
        )
        .finally(() => {
          settled = true;
          active -= 1;
          pump();
        });
    });
  });
  pump();
  return {
    promise,
    cancel() {
      if (cancelled || settled) return;
      cancelled = true;
      rejectRequest(new Error("Translation cancelled"));
      if (started) void cancelTextTransform(id).catch(() => {});
    },
  };
}
