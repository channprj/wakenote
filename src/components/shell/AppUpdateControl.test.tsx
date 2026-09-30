// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AppUpdateControl } from "./AppUpdateControl";
import type { UpdateInfo, UpdateProgress } from "@/lib/app-update";

const api = vi.hoisted(() => ({
  supported: vi.fn(), check: vi.fn(), blocker: vi.fn(), install: vi.fn(),
  listen: vi.fn(), unlisten: vi.fn(), open: vi.fn(),
}));
vi.mock("@/lib/app-update", async (original) => ({
  ...await original<typeof import("@/lib/app-update")>(),
  supportsAppUpdates: api.supported, checkForAppUpdate: api.check,
  updateInstallBlocker: api.blocker, installAppUpdate: api.install,
  onAppUpdateProgress: api.listen, openUpdateRelease: api.open,
}));

const latest: UpdateInfo = {
  status: "up_to_date", currentVersion: __APP_VERSION__, latestVersion: __APP_VERSION__,
  canInstall: false, installReason: null, notes: "", releaseUrl: "https://github.com/channprj/wakenote/releases",
};
const available: UpdateInfo = { ...latest, status: "available", latestVersion: "0.261001.0", canInstall: true, notes: "<img src=x onerror=alert(1)>" };
function mount() { return render(<TooltipProvider><AppUpdateControl /></TooltipProvider>); }
function open() { fireEvent.click(screen.getByRole("button", { name: /Open updates/ })); }

beforeEach(() => {
  vi.resetAllMocks();
  api.supported.mockReturnValue(true);
  api.check.mockResolvedValue(latest);
  api.blocker.mockResolvedValue(null);
  api.listen.mockResolvedValue(api.unlisten);
  api.install.mockResolvedValue(undefined);
  api.open.mockResolvedValue(undefined);
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

describe("sidebar app updates", () => {
  it("checks on launch and labels the confirmed latest version", async () => {
    mount();
    expect(screen.getByRole("button", { name: /Checking/ })).toBeTruthy();
    await screen.findByRole("button", { name: /Up to date.*Open updates/ });
    expect(api.check).toHaveBeenCalledTimes(1);
    open();
    expect(screen.getByText("You're running the latest published version of WakeNote.")).toBeTruthy();
  });

  it("does not claim a browser preview is current and marks a build ahead of release as Latest", async () => {
    api.supported.mockReturnValue(false);
    const first = mount();
    expect(screen.getByRole("button", { name: /Desktop app/ })).toBeTruthy();
    expect(api.check).not.toHaveBeenCalled();
    first.unmount();
    api.supported.mockReturnValue(true);
    api.check.mockResolvedValue({ ...latest, status: "ahead" });
    mount();
    const control = await screen.findByRole("button", { name: /Latest.*Open updates/ });
    expect(control.querySelector('[data-slot="badge"]')?.getAttribute("data-tone")).toBe("success");
    expect(screen.queryByText("Ahead of release")).toBeNull();
    expect(screen.queryByText("Up to date")).toBeNull();
  });

  it("reports failed checks and retries without showing stale up-to-date status", async () => {
    mount();
    await screen.findByRole("button", { name: /Up to date.*Open updates/ });
    open();
    api.check.mockRejectedValueOnce("Offline");
    fireEvent.click(screen.getByRole("button", { name: "Check again" }));
    await waitFor(() => expect(screen.getByRole("dialog").textContent).toContain("Check failed"));
    expect(screen.queryByText("Up to date")).toBeNull();
    expect(screen.getByRole("alert").textContent).toBe("Offline");
    fireEvent.click(screen.getByRole("button", { name: "Check again" }));
    await waitFor(() => expect(screen.getByRole("dialog").textContent).toContain("Up to date"));
  });

  it("checks daily while open and retries a failed check after an hour", async () => {
    vi.useFakeTimers();
    mount();
    await act(async () => {});
    await act(async () => { vi.advanceTimersByTime(23 * 60 * 60 * 1000); });
    expect(api.check).toHaveBeenCalledTimes(1);
    api.check.mockRejectedValueOnce("Offline");
    await act(async () => { vi.advanceTimersByTime(60 * 60 * 1000); });
    expect(api.check).toHaveBeenCalledTimes(2);
    await act(async () => { vi.advanceTimersByTime(60 * 60 * 1000); });
    expect(api.check).toHaveBeenCalledTimes(3);
  });

  it("blocks installation during active work and treats release notes as text", async () => {
    api.check.mockResolvedValue(available);
    api.blocker.mockResolvedValue("Stop the meeting recording before installing.");
    mount();
    await screen.findByRole("button", { name: /Update available/ });
    open();
    await screen.findByText("Stop the meeting recording before installing.");
    expect((screen.getByRole("button", { name: "Install & restart" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(available.notes)).toBeTruthy();
    expect(screen.getByRole("dialog").querySelector("img")).toBeNull();
    expect(api.install).not.toHaveBeenCalled();
  });

  it("subscribes before download, shows progress, prevents duplicate installs and waits for restart", async () => {
    api.check.mockResolvedValue(available);
    let finish!: () => void;
    api.install.mockReturnValue(new Promise<void>((resolve) => { finish = resolve; }));
    let progress!: (value: UpdateProgress) => void;
    api.listen.mockImplementation(async (callback) => { progress = callback; return api.unlisten; });
    mount();
    await screen.findByRole("button", { name: /Update available/ });
    open();
    const install = screen.getByRole("button", { name: "Install & restart" }) as HTMLButtonElement;
    await waitFor(() => expect(install.disabled).toBe(false));
    fireEvent.click(install);
    fireEvent.click(install);
    await waitFor(() => expect(api.install).toHaveBeenCalledExactlyOnceWith("0.261001.0"));
    act(() => progress({ phase: "downloading", downloaded: 50, total: 100 }));
    expect(screen.getByRole("progressbar").getAttribute("value")).toBe("50");
    expect((screen.getByRole("button", { name: "Check again" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByRole("button", { name: "Close" })).toBeNull();
    await act(async () => finish());
    expect(screen.getByRole("status").textContent).toContain("Restarting WakeNote");
    expect(api.unlisten).toHaveBeenCalledOnce();
  });

  it("keeps the app usable and offers retry if native verification or final idle check fails", async () => {
    api.check.mockResolvedValue(available);
    api.install.mockRejectedValue("The installer checksum does not match.");
    mount();
    await screen.findByRole("button", { name: /Update available/ });
    open();
    const install = screen.getByRole("button", { name: "Install & restart" }) as HTMLButtonElement;
    await waitFor(() => expect(install.disabled).toBe(false));
    fireEvent.click(install);
    await screen.findByText("The installer checksum does not match.");
    await waitFor(() => expect(install.disabled).toBe(false));
    expect(screen.getByRole("button", { name: "Close" })).toBeTruthy();
    expect(api.unlisten).toHaveBeenCalledOnce();
  });

  it("offers the release page when the current architecture has no installer", async () => {
    api.check.mockResolvedValue({ ...available, canInstall: false, installReason: "No installer for this Mac." });
    mount();
    await screen.findByRole("button", { name: /Update available/ });
    open();
    expect(screen.getByText("No installer for this Mac.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "View release on GitHub" }));
    expect(api.open).toHaveBeenCalledExactlyOnceWith("0.261001.0");
    expect((screen.getByRole("button", { name: "Install & restart" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
