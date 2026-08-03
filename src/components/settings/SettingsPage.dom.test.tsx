// @vitest-environment jsdom

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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

Object.defineProperties(HTMLElement.prototype, {
  hasPointerCapture: {
    configurable: true,
    value: () => false,
  },
  setPointerCapture: {
    configurable: true,
    value: () => {},
  },
  releasePointerCapture: {
    configurable: true,
    value: () => {},
  },
});

beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: () => {},
  });
});

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
    onSuspendDictationShortcut: vi.fn(),
    onResumeDictationShortcut: vi.fn(),
    onPressedModifierShortcut: vi.fn().mockResolvedValue(null),
    onChooseSaveRoot: vi.fn(),
    onRevealSaveFolder: vi.fn(),
    onChooseModelDirectory: vi.fn(),
    onOpenDictionaryFile: vi.fn(),
    onReloadDictionaryFile: vi.fn(),
    onRequestAccessibilityPermission: vi.fn(),
    onRequestMicrophonePermission: vi.fn(),
    onRequestScreenRecordingPermission: vi.fn(),
    onVerifyModel: vi.fn(),
    onDownloadModel: vi.fn(),
    onCancelModelDownload: vi.fn(),
    onDeleteModel: vi.fn(),
    onSaveOpenRouterApiKey: vi.fn(),
    onDeleteOpenRouterApiKey: vi.fn(),
    onSaveOpenAiApiKey: vi.fn(),
    onDeleteOpenAiApiKey: vi.fn(),
  };
}

describe("SettingsPage interactions", () => {
  it("keeps dictation controls unavailable until shortcut dictation is enabled", async () => {
    const user = userEvent.setup();
    const actions = makeActions();

    render(
      <SettingsPage
        section="dictation"
        onSectionChange={() => {}}
        snapshot={mockSnapshot()}
        actions={actions}
      />,
    );

    expect(
      screen
        .getByRole("button", { name: "Dictation shortcut" })
        .hasAttribute("disabled"),
    ).toBe(true);
    expect(
      screen
        .getByRole("combobox", { name: "Dictation language" })
        .hasAttribute("disabled"),
    ).toBe(true);
    for (const name of [
      "Dictation model",
      "Start sound",
      "Stop sound",
      "Cue volume",
      "Bubble position",
    ]) {
      expect(
        screen.getByRole("combobox", { name }).hasAttribute("disabled"),
      ).toBe(true);
    }
    for (const name of ["Copy result to clipboard", "Remove trailing space"]) {
      expect(
        screen.getByRole("switch", { name }).hasAttribute("disabled"),
      ).toBe(true);
    }

    await user.click(
      screen.getByRole("switch", { name: "Enable shortcut dictation" }),
    );

    expect(actions.onPatch).toHaveBeenCalledWith({ dictation_enabled: true });
  });

  it("patches independent Dictation feedback and model preferences", async () => {
    const user = userEvent.setup();
    const actions = makeActions();
    const snapshot = mockSnapshot();
    snapshot.settings.dictation_enabled = true;
    const smallModel = snapshot.models.find(
      (model) => model.id === "whisper-small",
    );
    if (!smallModel) {
      throw new Error("whisper-small fixture missing");
    }
    smallModel.status = "ready";

    const view = render(
      <SettingsPage
        section="dictation"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );

    async function choose(name: string, option: string) {
      await user.click(screen.getByRole("combobox", { name }));
      await user.click(await screen.findByRole("option", { name: option }));
    }

    expect(screen.getAllByText("Default")).toHaveLength(3);
    expect(screen.queryByText("Original")).toBeNull();
    expect(screen.queryByText("Alternative")).toBeNull();

    await choose("Start sound", "Ding");
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_start_sound: "alternative",
    });

    await choose("Stop sound", "Ding");
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_stop_sound: "alternative",
    });

    await choose("End sound", "Stapling");
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_end_sound: "alternative",
    });

    await choose("Cue volume", "Large");
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_cue_volume: "large",
    });

    await choose("Bubble position", "Bottom right");
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_bubble_position: "bottom_right",
    });

    await choose("Dictation model", "Whisper Small");
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_model: "whisper-small",
    });

    await user.click(
      screen.getByRole("switch", { name: "Copy result to clipboard" }),
    );
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_copy_to_clipboard: false,
    });

    await user.click(
      screen.getByRole("switch", { name: "Remove trailing space" }),
    );
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_remove_trailing_space: true,
    });

    snapshot.settings.dictation_model = "whisper-small";
    view.rerender(
      <SettingsPage
        section="dictation"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );

    await choose("Dictation model", "Default transcription model");
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_model: "",
    });
  });

  it("suspends the active shortcut while capturing a physical key combination", async () => {
    const user = userEvent.setup();
    const actions = makeActions();
    const snapshot = mockSnapshot();
    snapshot.settings.dictation_enabled = true;

    render(
      <SettingsPage
        section="dictation"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );

    const shortcut = screen.getByRole("button", { name: "Dictation shortcut" });
    await user.click(shortcut);
    expect(actions.onSuspendDictationShortcut).toHaveBeenCalledOnce();

    fireEvent.keyDown(shortcut, {
      code: "ControlLeft",
      key: "Control",
      ctrlKey: true,
    });
    fireEvent.keyDown(shortcut, {
      code: "ShiftLeft",
      key: "Shift",
      ctrlKey: true,
      shiftKey: true,
    });
    fireEvent.keyDown(shortcut, {
      code: "ShiftLeft",
      key: "Shift",
      ctrlKey: true,
      shiftKey: true,
      repeat: true,
    });

    expect(actions.onPatch).toHaveBeenCalledOnce();
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_shortcut: "ctrl+shift",
    });
    await act(async () => {});
    expect(actions.onResumeDictationShortcut).toHaveBeenCalledOnce();
  });

  it("captures a modifier-only shortcut from native flags without DOM keydown", async () => {
    const user = userEvent.setup();
    const actions = makeActions();
    actions.onPressedModifierShortcut = vi
      .fn()
      .mockResolvedValueOnce("ctrl+shift")
      .mockResolvedValue(null);
    const snapshot = mockSnapshot();
    snapshot.settings.dictation_enabled = true;

    render(
      <SettingsPage
        section="dictation"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: "Dictation shortcut" }),
    );

    await waitFor(() => {
      expect(actions.onPatch).toHaveBeenCalledWith({
        dictation_shortcut: "ctrl+shift",
      });
    });
    expect(actions.onResumeDictationShortcut).toHaveBeenCalledOnce();
  });

  it("waits for keyup before saving a single physical modifier", async () => {
    const user = userEvent.setup();
    const actions = makeActions();
    const snapshot = mockSnapshot();
    snapshot.settings.dictation_enabled = true;

    render(
      <SettingsPage
        section="dictation"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );

    const shortcut = screen.getByRole("button", { name: "Dictation shortcut" });
    await user.click(shortcut);
    fireEvent.keyDown(shortcut, {
      code: "ControlLeft",
      key: "Control",
      ctrlKey: true,
    });
    expect(actions.onPatch).not.toHaveBeenCalled();

    fireEvent.keyUp(shortcut, {
      code: "ControlLeft",
      key: "Control",
      ctrlKey: false,
    });

    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_shortcut: "leftctrl",
    });
    await act(async () => {});
    expect(actions.onResumeDictationShortcut).toHaveBeenCalledOnce();
  });

  it("restores the active shortcut when key capture is cancelled", async () => {
    const user = userEvent.setup();
    const actions = makeActions();
    const snapshot = mockSnapshot();
    snapshot.settings.dictation_enabled = true;

    render(
      <SettingsPage
        section="dictation"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );

    const shortcut = screen.getByRole("button", { name: "Dictation shortcut" });
    await user.click(shortcut);
    fireEvent.keyDown(shortcut, { code: "Escape", key: "Escape" });

    expect(actions.onPatch).not.toHaveBeenCalled();
    expect(actions.onResumeDictationShortcut).toHaveBeenCalledOnce();
  });

  it("restores the active shortcut when the dictation settings unmount", async () => {
    const user = userEvent.setup();
    const actions = makeActions();
    const snapshot = mockSnapshot();
    snapshot.settings.dictation_enabled = true;
    const view = render(
      <SettingsPage
        section="dictation"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: "Dictation shortcut" }),
    );
    view.unmount();
    await act(async () => {});

    expect(actions.onResumeDictationShortcut).toHaveBeenCalledOnce();
  });

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

    await userEvent.click(
      screen.getByRole("switch", { name: "Launch at login" }),
    );
    expect(actions.onPatch).toHaveBeenCalledWith({ launch_at_login: true });
  });

  it("keeps merged microphone input on by default and patches the exact key", async () => {
    const actions = makeActions();
    const snapshot = mockSnapshot();
    snapshot.settings.capture_microphones = [
      { id: "input-1-wired", label: "Wired" },
      { id: "input-2-wireless", label: "Wireless" },
    ];
    render(
      <SettingsPage
        section="audio"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );

    const mergeSwitch = screen.getByRole("switch", {
      name: "Merge microphone inputs",
    });
    expect(mergeSwitch.getAttribute("aria-checked")).toBe("true");

    await userEvent.click(mergeSwitch);
    expect(actions.onPatch).toHaveBeenCalledWith({
      merge_microphone_inputs: false,
    });
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

    fireEvent.change(screen.getByLabelText("Save Root"), {
      target: { value: "/tmp/notes" },
    });
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
    expect(actions.onPatch).toHaveBeenCalledWith({
      auto_transcript_input_enabled: true,
    });
  });

  it("keeps the compact Dictionary editor in Dictation instead of Models", async () => {
    const user = userEvent.setup();
    const actions = makeActions();
    const snapshot = mockSnapshot();
    snapshot.settings.dictionary = [
      {
        id: "wake-note",
        term: "WakeNote",
        aliases: ["wake note"],
        enabled: true,
      },
    ];

    render(
      <SettingsPage
        section="dictation"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );

    await user.click(
      screen.getByRole("switch", { name: "Enable shared Dictionary" }),
    );
    expect(actions.onPatch).toHaveBeenCalledWith({ dictionary_enabled: false });

    await user.click(
      screen.getByRole("button", { name: "Edit Dictionary entry WakeNote" }),
    );
    fireEvent.change(screen.getByLabelText("Aliases"), {
      target: { value: "wake-note, WakeNote, wake-note, 웨이크노트" },
    });
    await user.click(
      screen.getByRole("button", { name: "Save Dictionary entry" }),
    );
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictionary: [
        {
          id: "wake-note",
          term: "WakeNote",
          aliases: ["wake-note", "웨이크노트"],
          enabled: true,
        },
      ],
    });

    await user.click(
      screen.getByRole("button", { name: "Delete Dictionary entry WakeNote" }),
    );
    expect(actions.onPatch).toHaveBeenCalledWith({ dictionary: [] });

    cleanup();
    render(
      <SettingsPage
        section="models"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );
    expect(screen.queryByText("Dictionary")).toBeNull();
  });

  it("saves and deletes OpenRouter and OpenAI credentials independently", async () => {
    const user = userEvent.setup();
    const actions = makeActions();
    const snapshot = mockSnapshot();
    snapshot.openrouter_key_configured = true;
    snapshot.openai_key_configured = true;

    render(
      <SettingsPage
        section="integrations"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );

    const openRouterCard = screen
      .getByText("OpenRouter")
      .closest('[data-slot="card"]');
    const openAiCard = screen.getByText("OpenAI").closest('[data-slot="card"]');
    if (
      !(openRouterCard instanceof HTMLElement) ||
      !(openAiCard instanceof HTMLElement)
    ) {
      throw new Error("API key settings cards not found");
    }

    await user.type(
      within(openRouterCard).getByLabelText("OpenRouter API Key"),
      "sk-or-test",
    );
    await user.click(
      within(openRouterCard).getByRole("button", { name: /Save/ }),
    );
    expect(actions.onSaveOpenRouterApiKey).toHaveBeenCalledWith("sk-or-test");
    await user.click(
      within(openRouterCard).getByRole("button", { name: /Delete/ }),
    );
    expect(actions.onDeleteOpenRouterApiKey).toHaveBeenCalledOnce();

    await user.type(
      within(openAiCard).getByLabelText("OpenAI API Key"),
      "sk-openai-test",
    );
    await user.click(within(openAiCard).getByRole("button", { name: /Save/ }));
    expect(actions.onSaveOpenAiApiKey).toHaveBeenCalledWith("sk-openai-test");
    await user.click(
      within(openAiCard).getByRole("button", { name: /Delete/ }),
    );
    expect(actions.onDeleteOpenAiApiKey).toHaveBeenCalledOnce();
  });

  it("only emits solid caption colors and both opacity boundaries", async () => {
    const user = userEvent.setup();
    const actions = makeActions();
    const snapshot = mockSnapshot();

    const view = render(
      <SettingsPage
        section="integrations"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );

    await user.click(
      screen.getByRole("combobox", { name: "Overlay background" }),
    );
    await user.click(await screen.findByRole("option", { name: "White" }));
    expect(actions.onPatch).toHaveBeenCalledWith({
      floating_overlay_background_color: "#ffffff",
      floating_overlay_text_color: "#000000",
    });

    snapshot.settings.floating_overlay_background_color = "#ffffff";
    view.rerender(
      <SettingsPage
        section="integrations"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );
    await user.click(
      screen.getByRole("combobox", { name: "Overlay background" }),
    );
    await user.click(await screen.findByRole("option", { name: "Black" }));
    expect(actions.onPatch).toHaveBeenCalledWith({
      floating_overlay_background_color: "#000000",
      floating_overlay_text_color: "#ffffff",
    });

    const opacity = screen.getByRole("slider", {
      name: "Overlay background opacity",
    });
    fireEvent.keyDown(opacity, { key: "Home" });
    expect(actions.onPatch).toHaveBeenCalledWith({
      floating_overlay_background_opacity: 0,
    });
    fireEvent.keyDown(opacity, { key: "End" });
    expect(actions.onPatch).toHaveBeenCalledWith({
      floating_overlay_background_opacity: 100,
    });
  });

  it("only emits solid Dictation colors and both opacity boundaries", async () => {
    const user = userEvent.setup();
    const actions = makeActions();
    const snapshot = mockSnapshot();

    const view = render(
      <SettingsPage
        section="dictation"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );

    await user.click(
      screen.getByRole("combobox", { name: "Bubble background" }),
    );
    await user.click(await screen.findByRole("option", { name: "White" }));
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_bubble_background_color: "#ffffff",
    });

    snapshot.settings.dictation_bubble_background_color = "#ffffff";
    view.rerender(
      <SettingsPage
        section="dictation"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={actions}
      />,
    );
    await user.click(
      screen.getByRole("combobox", { name: "Bubble background" }),
    );
    await user.click(await screen.findByRole("option", { name: "Black" }));
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_bubble_background_color: "#000000",
    });

    const opacity = screen.getByRole("slider", {
      name: "Bubble background opacity",
    });
    fireEvent.keyDown(opacity, { key: "Home" });
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_bubble_background_opacity: 0,
    });
    fireEvent.keyDown(opacity, { key: "End" });
    expect(actions.onPatch).toHaveBeenCalledWith({
      dictation_bubble_background_opacity: 100,
    });
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

    const position = screen.getByRole("combobox", {
      name: "Floating overlay position",
    });
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
    grid.style.setProperty("--masonry-row-size", "1px");
    grid.style.setProperty("--masonry-card-gap", "12px");
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
    ).toEqual(new Set([grid, ...enabledCards]));
    expect(frames.callbacks.size).toBe(1);
    expect(frames.request).toHaveBeenCalledOnce();

    act(() => frames.flush());

    expect(grid.dataset.masonryReady).toBe("true");
    for (const card of enabledCards) {
      expect(card.style.gridRowEnd).toBe("span 112");
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
      expect(card.style.gridRowEnd).toBe("span 112");
    }
  });
});
