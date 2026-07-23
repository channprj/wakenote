// @vitest-environment jsdom

import { act, cleanup, render } from "@testing-library/react";
import { useLayoutEffect, type CSSProperties } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  masonryRowSpan,
  measureMasonryGrid,
  resetMasonryGrid,
  useMasonryGrid,
} from "./use-masonry-grid";

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
    const pending = [...this.callbacks.entries()];
    this.callbacks.clear();
    for (const [, callback] of pending) {
      callback(0);
    }
  }
}

class ControlledResizeObserver implements ResizeObserver {
  static instances: ControlledResizeObserver[] = [];

  readonly observe = vi.fn();
  readonly unobserve = vi.fn();
  readonly disconnect = vi.fn();

  constructor(private readonly callback: ResizeObserverCallback) {
    ControlledResizeObserver.instances.push(this);
  }

  takeRecords(): ResizeObserverEntry[] {
    return [];
  }

  notify(): void {
    this.callback([], this);
  }
}

let animationFrames: ControlledAnimationFrames;

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

function setHeight(item: HTMLElement, height: number): void {
  vi.spyOn(item, "getBoundingClientRect").mockReturnValue(rect(height));
}

function createGrid(...heights: number[]): {
  grid: HTMLDivElement;
  items: HTMLDivElement[];
} {
  const grid = document.createElement("div");
  grid.style.setProperty("--masonry-row-size", "4px");
  grid.style.rowGap = "12px";
  const items = heights.map((height) => {
    const item = document.createElement("div");
    setHeight(item, height);
    grid.append(item);
    return item;
  });
  return { grid, items };
}

function MasonryFixture({
  itemIds = ["1", "2"],
  onLayoutRead,
}: {
  itemIds?: string[];
  onLayoutRead?: (masonryReady: string | undefined) => void;
}) {
  const itemIdentity = JSON.stringify(itemIds);
  const ref = useMasonryGrid<HTMLDivElement>(
    itemIds.length,
    itemIdentity,
  );
  const style = {
    "--masonry-row-size": "4px",
    rowGap: "12px",
  } as CSSProperties;

  useLayoutEffect(() => {
    onLayoutRead?.(ref.current?.dataset.masonryReady);
  }, [itemIdentity, onLayoutRead, ref]);

  return (
    <div data-testid="grid" ref={ref} style={style}>
      {itemIds.map((itemId) => (
        <div key={itemId} data-testid={`item-${itemId}`} />
      ))}
    </div>
  );
}

beforeEach(() => {
  ControlledResizeObserver.instances = [];
  animationFrames = new ControlledAnimationFrames();
  vi.stubGlobal("ResizeObserver", ControlledResizeObserver);
  vi.stubGlobal("requestAnimationFrame", animationFrames.request);
  vi.stubGlobal("cancelAnimationFrame", animationFrames.cancel);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("masonryRowSpan", () => {
  it("calculates row spans and rejects invalid inputs", () => {
    expect(masonryRowSpan(100, 4, 12)).toBe(7);
    expect(masonryRowSpan(116, 4, 12)).toBe(8);

    expect(masonryRowSpan(0, 4, 12)).toBeNull();
    expect(masonryRowSpan(Number.NaN, 4, 12)).toBeNull();
    expect(masonryRowSpan(Number.POSITIVE_INFINITY, 4, 12)).toBeNull();
    expect(masonryRowSpan(100, 0, 12)).toBeNull();
    expect(masonryRowSpan(100, -4, 12)).toBeNull();
    expect(masonryRowSpan(100, Number.NaN, 12)).toBeNull();
    expect(masonryRowSpan(100, 4, -1)).toBeNull();
    expect(masonryRowSpan(100, 4, Number.POSITIVE_INFINITY)).toBeNull();
  });
});

describe("masonry grid measurement", () => {
  it("resets readiness and direct item spans without touching descendants", () => {
    const grid = document.createElement("div");
    const item = document.createElement("div");
    const descendant = document.createElement("div");
    grid.dataset.masonryReady = "true";
    item.style.gridRowEnd = "span 7";
    descendant.style.gridRowEnd = "span 3";
    item.append(descendant);
    grid.append(item);

    resetMasonryGrid(grid);

    expect(grid.dataset.masonryReady).toBeUndefined();
    expect(item.style.gridRowEnd).toBe("");
    expect(descendant.style.gridRowEnd).toBe("span 3");
  });

  it("activates masonry and applies measured spans to direct items", () => {
    const { grid, items } = createGrid(100, 52);

    expect(measureMasonryGrid(grid)).toBe(true);

    expect(items[0].style.gridRowEnd).toBe("span 7");
    expect(items[1].style.gridRowEnd).toBe("span 4");
    expect(grid.dataset.masonryReady).toBe("true");
  });

  it("accepts complete decimal pixel tokens with surrounding whitespace", () => {
    const { grid, items } = createGrid(6);
    grid.style.setProperty("--masonry-row-size", " 2.5px ");
    grid.style.rowGap = "1.5px";

    expect(measureMasonryGrid(grid)).toBe(true);
    expect(items[0].style.gridRowEnd).toBe("span 2");
    expect(grid.dataset.masonryReady).toBe("true");
  });

  it("rejects a row-size token with trailing garbage", () => {
    const { grid, items } = createGrid(100);
    grid.style.setProperty("--masonry-row-size", "4garbage");

    expect(measureMasonryGrid(grid)).toBe(false);
    expect(items[0].style.gridRowEnd).toBe("");
    expect(grid.dataset.masonryReady).toBeUndefined();
  });

  it("rejects an unsupported row-size unit", () => {
    const { grid, items } = createGrid(100);
    grid.style.setProperty("--masonry-row-size", "0.25rem");

    expect(measureMasonryGrid(grid)).toBe(false);
    expect(items[0].style.gridRowEnd).toBe("");
    expect(grid.dataset.masonryReady).toBeUndefined();
  });

  it("rejects a percentage row gap", () => {
    const { grid, items } = createGrid(100);
    grid.style.rowGap = "10%";

    expect(measureMasonryGrid(grid)).toBe(false);
    expect(items[0].style.gridRowEnd).toBe("");
    expect(grid.dataset.masonryReady).toBeUndefined();
  });

  it("returns to the ordinary-grid fallback when an item measurement throws", () => {
    const { grid, items } = createGrid(100, 52);
    grid.dataset.masonryReady = "true";
    items[0].style.gridRowEnd = "span 7";
    items[1].style.gridRowEnd = "span 4";
    vi.spyOn(items[1], "getBoundingClientRect").mockImplementation(() => {
      throw new Error("layout unavailable");
    });

    let measured: boolean | undefined;
    expect(() => {
      measured = measureMasonryGrid(grid);
    }).not.toThrow();
    expect(measured).toBe(false);
    expect(grid.dataset.masonryReady).toBeUndefined();
    expect(items[0].style.gridRowEnd).toBe("");
    expect(items[1].style.gridRowEnd).toBe("");
  });
});

describe("useMasonryGrid", () => {
  it("observes current items and coalesces repeated notifications", () => {
    const { getByTestId } = render(<MasonryFixture />);
    const grid = getByTestId("grid");
    const items = [getByTestId("item-1"), getByTestId("item-2")];
    setHeight(items[0], 100);
    setHeight(items[1], 52);
    const observer = ControlledResizeObserver.instances[0];

    expect(observer.observe).toHaveBeenCalledTimes(3);
    expect(observer.observe).toHaveBeenNthCalledWith(1, grid);
    expect(observer.observe).toHaveBeenNthCalledWith(2, items[0]);
    expect(observer.observe).toHaveBeenNthCalledWith(3, items[1]);
    expect(animationFrames.callbacks.size).toBe(1);

    act(() => animationFrames.flush());
    act(() => {
      observer.notify();
      observer.notify();
    });

    expect(animationFrames.callbacks.size).toBe(1);
    expect(animationFrames.request).toHaveBeenCalledTimes(2);
  });

  it("remeasures a changed card height without scheduling duplicate frames", () => {
    const { getByTestId } = render(<MasonryFixture />);
    const grid = getByTestId("grid");
    const items = [getByTestId("item-1"), getByTestId("item-2")];
    const firstRect = vi
      .spyOn(items[0], "getBoundingClientRect")
      .mockReturnValue(rect(100));
    setHeight(items[1], 52);
    const observer = ControlledResizeObserver.instances[0];

    act(() => animationFrames.flush());

    expect(grid.dataset.masonryReady).toBe("true");
    expect(items[0].style.gridRowEnd).toBe("span 7");
    expect(items[1].style.gridRowEnd).toBe("span 4");
    expect(animationFrames.request).toHaveBeenCalledOnce();

    firstRect.mockReturnValue(rect(148));
    act(() => {
      observer.notify();
      observer.notify();
    });

    expect(animationFrames.callbacks.size).toBe(1);
    expect(animationFrames.request).toHaveBeenCalledTimes(2);

    act(() => animationFrames.flush());

    expect(items[0].style.gridRowEnd).toBe("span 10");
    expect(items[1].style.gridRowEnd).toBe("span 4");
    expect(grid.dataset.masonryReady).toBe("true");
  });

  it("disconnects, cancels a pending frame, and resets on unmount", () => {
    const { getByTestId, unmount } = render(<MasonryFixture />);
    const grid = getByTestId("grid");
    const observer = ControlledResizeObserver.instances[0];
    grid.dataset.masonryReady = "true";

    unmount();

    expect(observer.disconnect).toHaveBeenCalledOnce();
    expect(animationFrames.cancel).toHaveBeenCalledOnce();
    expect(animationFrames.callbacks.size).toBe(0);
    expect(grid.dataset.masonryReady).toBeUndefined();
  });

  it("rebinds and remeasures replaced children when item identity changes at the same count", () => {
    const layoutReadiness = vi.fn();
    const { getByTestId, rerender } = render(
      <MasonryFixture
        itemIds={["alpha", "beta"]}
        onLayoutRead={layoutReadiness}
      />,
    );
    const grid = getByTestId("grid");
    const originalItems = [
      getByTestId("item-alpha"),
      getByTestId("item-beta"),
    ];
    setHeight(originalItems[0], 100);
    setHeight(originalItems[1], 52);

    act(() => animationFrames.flush());

    expect(grid.dataset.masonryReady).toBe("true");
    expect(originalItems[0].style.gridRowEnd).toBe("span 7");
    expect(originalItems[1].style.gridRowEnd).toBe("span 4");

    const originalObserver = ControlledResizeObserver.instances[0];
    layoutReadiness.mockClear();
    rerender(
      <MasonryFixture
        itemIds={["gamma", "delta"]}
        onLayoutRead={layoutReadiness}
      />,
    );

    const replacementItems = [
      getByTestId("item-gamma"),
      getByTestId("item-delta"),
    ];
    setHeight(replacementItems[0], 84);
    setHeight(replacementItems[1], 116);

    expect(layoutReadiness).toHaveBeenCalledOnce();
    expect(layoutReadiness).toHaveBeenCalledWith(undefined);
    expect(grid.dataset.masonryReady).toBeUndefined();
    expect(originalObserver.disconnect).toHaveBeenCalledOnce();
    expect(ControlledResizeObserver.instances).toHaveLength(2);
    expect(ControlledResizeObserver.instances[1].observe).toHaveBeenCalledWith(
      replacementItems[0],
    );
    expect(ControlledResizeObserver.instances[1].observe).toHaveBeenCalledWith(
      replacementItems[1],
    );

    act(() => animationFrames.flush());

    expect(grid.dataset.masonryReady).toBe("true");
    expect(replacementItems[0].style.gridRowEnd).toBe("span 6");
    expect(replacementItems[1].style.gridRowEnd).toBe("span 8");
  });

  it("leaves the ordinary-grid fallback untouched without ResizeObserver", () => {
    vi.stubGlobal("ResizeObserver", undefined);

    const { getByTestId } = render(<MasonryFixture />);

    expect(animationFrames.request).not.toHaveBeenCalled();
    expect(getByTestId("grid").dataset.masonryReady).toBeUndefined();
  });
});
