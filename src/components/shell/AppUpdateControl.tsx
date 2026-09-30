import { ArrowDownToLineIcon, CheckIcon, CircleAlertIcon, LoaderCircleIcon, RefreshCwIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { useAppUpdate } from "@/hooks/use-app-update";
import { openUpdateRelease, updateError, updateInstallBlocker } from "@/lib/app-update";
import { Button } from "@/components/ui/button";
import { StatusBadge } from "@/components/ui/status-badge";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { StatusTone } from "@/lib/status-summary";

export function AppUpdateControl() {
  const update = useAppUpdate();
  const [open, setOpen] = useState(false);
  const [blocker, setBlocker] = useState<string | null>(null);
  const [readinessChecked, setReadinessChecked] = useState(false);
  const [releaseError, setReleaseError] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !update.info?.canInstall || update.installing) return;
    let disposed = false;
    let pending = false;
    const refresh = async () => {
      if (pending) return;
      pending = true;
      try {
        const reason = await updateInstallBlocker();
        if (!disposed) setBlocker(reason);
      } catch (error) {
        if (!disposed) setBlocker(updateError(error));
      } finally {
        pending = false;
        if (!disposed) setReadinessChecked(true);
      }
    };
    setReadinessChecked(false);
    void refresh();
    const timer = window.setInterval(() => void refresh(), 2000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [open, update.info, update.installing]);

  let label = "Not checked";
  let tone: StatusTone = "neutral";
  let Icon = RefreshCwIcon;
  if (!update.supported) label = "Desktop app";
  else if (update.installing) { label = "Updating"; tone = "primary"; Icon = LoaderCircleIcon; }
  else if (update.checking) { label = "Checking"; Icon = LoaderCircleIcon; }
  else if (update.checkFailed) { label = "Check failed"; tone = "warning"; Icon = CircleAlertIcon; }
  else if (update.info?.status === "available") { label = "Update available"; tone = "primary"; Icon = ArrowDownToLineIcon; }
  else if (update.info?.status === "up_to_date") { label = "Up to date"; tone = "success"; Icon = CheckIcon; }
  else if (update.info?.status === "ahead") { label = "Ahead of release"; Icon = CheckIcon; }
  else if (update.info?.status === "no_release") label = "No release yet";

  const progressLabel = update.progress?.phase === "verifying" ? "Verifying update…"
    : update.progress?.phase === "restarting" ? "Restarting WakeNote…"
    : update.progress?.total ? `Downloading… ${Math.min(100, Math.floor(update.progress.downloaded / update.progress.total * 100))}%`
    : "Preparing download…";
  const explanation = !update.supported ? "Update checks are available in the installed WakeNote desktop app."
    : update.checking ? "Checking the latest public release on GitHub."
    : update.checkFailed ? "The latest release could not be confirmed. Check your connection and try again."
    : update.info?.status === "available" ? `WakeNote ${update.info.latestVersion} is available. Install it to restart with the new version.`
    : update.info?.status === "up_to_date" ? "You're running the latest published version of WakeNote."
    : update.info?.status === "ahead" ? "This build is newer than the latest published release."
    : update.info?.status === "no_release" ? "No public release is available yet."
    : "WakeNote automatically checks for new releases once a day.";

  return (
    <Dialog open={open} onOpenChange={(value) => { if (!update.installing) setOpen(value); }}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DialogTrigger asChild>
            <button type="button" className="app-update-control" aria-label={`WakeNote v${__APP_VERSION__}: ${label}. Open updates`}>
              <span className="app-sidebar__version">v{__APP_VERSION__}</span>
              <StatusBadge tone={tone} className="app-update-control__chip">
                <Icon className={update.checking || update.installing ? "animate-spin" : undefined} aria-hidden="true" />
                <span className="app-update-control__label">{label}</span>
              </StatusBadge>
            </button>
          </DialogTrigger>
        </TooltipTrigger>
        <TooltipContent side="right">v{__APP_VERSION__} · {label}</TooltipContent>
      </Tooltip>
      <DialogContent showCloseButton={!update.installing} className="app-update-dialog">
        <DialogHeader>
          <DialogTitle>WakeNote updates</DialogTitle>
          <DialogDescription>{explanation}</DialogDescription>
        </DialogHeader>
        <div className="app-update-dialog__content">
          <div className="app-update-dialog__versions">
            <span>Installed <strong>v{update.info?.currentVersion ?? __APP_VERSION__}</strong></span>
            <StatusBadge tone={tone}>{label}</StatusBadge>
          </div>
          {update.checkedAt && !update.checkFailed ? <p className="text-muted-foreground">Last checked {new Date(update.checkedAt).toLocaleString()}</p> : null}
          {update.error ? <p role="alert" className="text-destructive">{update.error}</p> : null}
          {releaseError ? <p role="alert" className="text-destructive">{releaseError}</p> : null}
          {update.supported ? <Button variant="link" className="justify-self-start p-0" disabled={update.installing} onClick={() => {
            setReleaseError(null);
            void openUpdateRelease(update.info?.latestVersion ?? null).catch((error) => setReleaseError(updateError(error)));
          }}>View release on GitHub</Button> : null}
          {update.info?.status === "available" ? <>
            {update.info.installReason ? <p>{update.info.installReason}</p> : null}
            {blocker && !update.installing ? <p role="status">{blocker}</p> : null}
            {update.info.notes ? <details><summary>What's new in v{update.info.latestVersion}</summary><pre className="app-update-dialog__notes">{update.info.notes}</pre></details> : null}
          </> : null}
          {update.installing ? <div role="status" aria-live="polite">
            <p>{progressLabel}</p>
            {update.progress?.phase === "downloading" && update.progress.total > 0 ? <progress aria-label="Update download" value={update.progress.downloaded} max={update.progress.total} /> : null}
            <p>WakeNote will reopen automatically. Your recordings and settings stay in place.</p>
          </div> : <p className="text-muted-foreground">Checks automatically once a day. Installation restarts the app.</p>}
        </div>
        <DialogFooter>
          <Button variant="outline" disabled={!update.supported || update.checking || update.installing} onClick={() => void update.check()}>
            <RefreshCwIcon aria-hidden="true" />Check again
          </Button>
          {update.info?.status === "available" ? <Button disabled={!update.info.canInstall || !readinessChecked || Boolean(blocker) || update.checkFailed || update.checking || update.installing} onClick={() => void update.install()}>
            <ArrowDownToLineIcon aria-hidden="true" />{update.installing ? progressLabel : "Install & restart"}
          </Button> : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
