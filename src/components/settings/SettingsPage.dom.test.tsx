// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockSnapshot } from "@/lib/app-state";
import { SettingsPage } from "./SettingsPage";
import type { SettingsActions } from "./types";

class TestResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

class ControlledResizeObserver implements ResizeObserver {
  static instances: ControlledResizeObserver[] = [];

  readonly observe = vi.fn();
  readonly unobserve = vi.fn();
  readonly disconnect = vi.fn();

  constructor(_callback: ResizeObserverCallback) {
    ControlledResizeObserver.instances.push(this);
  }

  takeRecords(): ResizeObserverEntry[] {
    return [];
  }
}

class ControlledAnimationFrames {
  private nextId = 1;
  readonly callbacks = new Map<number, FrameRequestCallback>();
  readonly request = vi.fn((callback: FrameRequestCallback) => {
    const id = this.nextId;
    this.nextId += 1;
    this.callbacks.set(id, callback);
    return id;
  });
  readonly cancel = vi.fn((id: number) => {
    this.callbacks.delete(id);
  });

  flush(): void {
    const pending = [...this.callbacks.values()];
    this.callbacks.clear();
    for (const callback of pending) {
      callback(0);
    }
  }
}

globalThis.ResizeObserver = TestResizeObserver as typeof ResizeObserver;

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  ControlledResizeObserver.instances = [];
  Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView");
});

function rect(height: number): DOMRect {
  return {
    x: 0,
    y: 0,
    width: 0,
    height,
    top: 0,
    right: 0,
    bottom: height,
    left: 0,
    toJSON: () => ({}),
  };
}

function directSettingsCards(grid: HTMLElement): HTMLElement[] {
  return Array.from(grid.children).filter(
    (child): child is HTMLElement =>
      child instanceof HTMLElement && child.matches('[data-slot="card"]'),
  );
}

function makeActions(): SettingsActions {
  return {
    onPatch: vi.fn(),
    onChooseSaveRoot: vi.fn(),
    onRevealSaveFolder: vi.fn(),
    onChooseModelDirectory: vi.fn(),
    onRequestMicrophonePermission: vi.fn(),
    onRequestScreenRecordingPermission: vi.fn(),
    onVerifyModel: vi.fn(),
    onDownloadModel: vi.fn(),
    onCancelModelDownload: vi.fn(),
    onDeleteModel: vi.fn(),
    onSaveOpenRouterApiKey: vi.fn(),
    onDeleteOpenRouterApiKey: vi.fn(),
  };
}

describe("SettingsPage interactions", () => {
  it("keeps a controlled active tab visible inside the compact tab scroller", () => {
    const scrollIntoView = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: scrollIntoView,
    });

    render(
      <SettingsPage
        section="advanced"
        onSectionChange={() => {}}
        snapshot={mockSnapshot()}
        actions={makeActions()}
      />,
    );

    expect(scrollIntoView).toHaveBeenCalledWith({
      block: "nearest",
      inline: "nearest",
    });
  });

  it("reports settings tab changes through the controlled section contract", async () => {
    const onSectionChange = vi.fn();
    render(
      <SettingsPage
        section="general"
        onSectionChange={onSectionChange}
        snapshot={mockSnapshot()}
        actions={makeActions()}
      />,
    );

    await userEvent.click(screen.getByRole("tab", { name: "Audio" }));
    expect(onSectionChange).toHaveBeenCalledWith("audio");
  });

  it("preserves the exact General patch key", async () => {
    const actions = makeActions();
    render(
      <SettingsPage
        section="general"
        onSectionChange={() => {}}
        snapshot={mockSnapshot()}
        actions={actions}
      />,
    );

    await userEvent.click(screen.getByRole("switch", { name: "Launch at login" }));
    expect(actions.onPatch).toHaveBeenCalledWith({ launch_at_login: true });
  });

  it("preserves the Save Root patch key", () => {
    const actions = makeActions();
    render(
      <SettingsPage
        section="storage"
        onSectionChange={() => {}}
        snapshot={mockSnapshot()}
        actions={actions}
      />,
    );

    fireEvent.change(screen.getByLabelText("Save Root"), { target: { value: "/tmp/notes" } });
    expect(actions.onPatch).toHaveBeenCalledWith({ save_root: "/tmp/notes" });
  });

  it("preserves the auto-type integration patch key", async () => {
    const actions = makeActions();
    render(
      <SettingsPage
        section="integrations"
        onSectionChange={() => {}}
        snapshot={mockSnapshot()}
        actions={actions}
      />,
    );

    await userEvent.click(
      screen.getByRole("switch", { name: "Auto-type transcripts into cursor" }),
    );
    expect(actions.onPatch).toHaveBeenCalledWith({ auto_transcript_input_enabled: true });
  });

  it("disables the floating overlay position when the overlay is hidden", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.show_floating_overlay = false;

    render(
      <SettingsPage
        section="integrations"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={makeActions()}
      />,
    );

    const position = screen.getByRole("combobox", { name: "Floating overlay position" });
    expect((position as HTMLButtonElement).disabled).toBe(true);
  });

  it("keeps later Audio controls mounted when system sources are removed", () => {
    ControlledResizeObserver.instances = [];
    const frames = new ControlledAnimationFrames();
    vi.stubGlobal("ResizeObserver", ControlledResizeObserver);
    vi.stubGlobal("requestAnimationFrame", frames.request);
    vi.stubGlobal("cancelAnimationFrame", frames.cancel);

    const actions = makeActions();
    const enabled = mockSnapshot();
    enabled.settings.system_audio_enabled = true;
    const { container, rerender } = render(
      <SettingsPage
        section="audio"
        onSectionChange={() => {}}
        snapshot={enabled}
        actions={actions}
      />,
    );

    const grid = container.querySelector<HTMLElement>(
      '[data-slot="settings-grid"]',
    );
    if (!grid) {
      throw new Error("Audio settings grid not found");
    }
    grid.style.setProperty("--masonry-row-size", "4px");
    grid.style.rowGap = "12px";
    const enabledCards = directSettingsCards(grid);
    for (const card of enabledCards) {
      vi.spyOn(card, "getBoundingClientRect").mockReturnValue(rect(100));
    }

    expect(enabledCards).toHaveLength(5);
    expect(screen.getByText("Recognized system sources")).toBeTruthy();
    expect(screen.getByText("Chunk timing")).toBeTruthy();
    const initialInstances = new Set(ControlledResizeObserver.instances);
    const initialGridObservers = ControlledResizeObserver.instances.filter(
      (observer) =>
        observer.observe.mock.calls.some(([target]) => target === grid),
    );
    expect(initialGridObservers).toHaveLength(1);
    const initialObserver = initialGridObservers[0];
    expect(initialObserver.observe).toHaveBeenCalledTimes(6);
    expect(
      new Set(initialObserver.observe.mock.calls.map(([target]) => target)),
    ).toEqual(
      new Set([grid, ...enabledCards]),
    );
    expect(frames.callbacks.size).toBe(1);
    expect(frames.request).toHaveBeenCalledOnce();

    act(() => frames.flush());

    expect(grid.dataset.masonryReady).toBe("true");
    for (const card of enabledCards) {
      expect(card.style.gridRowEnd).toBe("span 7");
    }

    const disabled = mockSnapshot();
    disabled.settings.system_audio_enabled = false;
    rerender(
      <SettingsPage
        section="audio"
        onSectionChange={() => {}}
        snapshot={disabled}
        actions={actions}
      />,
    );

    const currentGrid = container.querySelector<HTMLElement>(
      '[data-slot="settings-grid"]',
    );
    expect(currentGrid).toBe(grid);
    const remainingCards = directSettingsCards(grid);

    expect(remainingCards).toHaveLength(4);
    expect(screen.queryByText("Recognized system sources")).toBeNull();
    expect(screen.getByText("Chunk timing")).toBeTruthy();
    expect(initialObserver.disconnect).toHaveBeenCalledOnce();
    const replacementGridObservers = ControlledResizeObserver.instances.filter(
      (observer) =>
        !initialInstances.has(observer) &&
        observer.observe.mock.calls.some(([target]) => target === grid),
    );
    expect(replacementGridObservers).toHaveLength(1);
    const replacementObserver = replacementGridObservers[0];
    expect(replacementObserver.observe).toHaveBeenCalledTimes(5);
    expect(
      new Set(replacementObserver.observe.mock.calls.map(([target]) => target)),
    ).toEqual(new Set([grid, ...remainingCards]));
    expect(frames.callbacks.size).toBe(1);
    expect(frames.request).toHaveBeenCalledTimes(2);
    expect(grid.dataset.masonryReady).toBeUndefined();

    act(() => frames.flush());

    expect(grid.dataset.masonryReady).toBe("true");
    for (const card of remainingCards) {
      expect(card.style.gridRowEnd).toBe("span 7");
    }
  });
});
