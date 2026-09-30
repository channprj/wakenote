import { useCallback, useEffect, useRef, useState } from "react";
import {
  checkForAppUpdate, installAppUpdate, onAppUpdateProgress,
  supportsAppUpdates, updateError, updateInstallReadiness,
  UPDATE_CHECK_INTERVAL, UPDATE_RETRY_INTERVAL,
  type UpdateInfo, type UpdateProgress,
} from "@/lib/app-update";

export function useAppUpdate() {
  const supported = supportsAppUpdates();
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [checkFailed, setCheckFailed] = useState(false);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [checkedAt, setCheckedAt] = useState<number | null>(null);
  const checkingRef = useRef(false);
  const installingRef = useRef(false);
  const nextCheck = useRef(0);
  const mounted = useRef(false);

  const check = useCallback(async () => {
    if (!supported || checkingRef.current || installingRef.current) return;
    checkingRef.current = true;
    setChecking(true);
    setError(null);
    try {
      const result = await checkForAppUpdate();
      if (!mounted.current) return;
      setInfo(result);
      setCheckedAt(Date.now());
      setCheckFailed(false);
      nextCheck.current = Date.now() + UPDATE_CHECK_INTERVAL;
    } catch (failure) {
      if (!mounted.current) return;
      setError(updateError(failure));
      setCheckFailed(true);
      nextCheck.current = Date.now() + UPDATE_RETRY_INTERVAL;
    } finally {
      checkingRef.current = false;
      if (mounted.current) setChecking(false);
    }
  }, [supported]);

  useEffect(() => {
    mounted.current = true;
    const tick = () => { if (Date.now() >= nextCheck.current) void check(); };
    tick();
    const timer = window.setInterval(tick, UPDATE_RETRY_INTERVAL);
    // Sleeping Macs may suspend timers. Recheck overdue status on return.
    window.addEventListener("focus", tick);
    return () => {
      mounted.current = false;
      window.clearInterval(timer);
      window.removeEventListener("focus", tick);
    };
  }, [check]);

  const install = useCallback(async () => {
    if (!info?.canInstall || !info.latestVersion || checkFailed || checkingRef.current || installingRef.current) return;
    installingRef.current = true;
    setInstalling(true);
    setError(null);
    setProgress({ phase: "downloading", downloaded: 0, total: 0 });
    let unlisten: (() => void) | undefined;
    try {
      // Capture input and in-flight transcription are handled natively.
      const { blocker } = await updateInstallReadiness();
      if (blocker) throw new Error(blocker);
      unlisten = await onAppUpdateProgress((value) => {
        if (mounted.current) setProgress(value);
      });
      await installAppUpdate(info.latestVersion);
      // Native success requests app exit; keep the controls locked until then.
      if (mounted.current) setProgress({ phase: "restarting", downloaded: 0, total: 0 });
    } catch (failure) {
      installingRef.current = false;
      if (mounted.current) {
        setInstalling(false);
        setProgress(null);
        setError(updateError(failure));
      }
    } finally {
      unlisten?.();
    }
  }, [info, checkFailed]);

  return { supported, info, checking, installing, error, checkFailed, progress, checkedAt, check, install };
}
