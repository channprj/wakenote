import { useEffect, useRef } from "react";
import type { RefObject } from "react";

function directItems(grid: HTMLElement): HTMLElement[] {
  return Array.from(grid.children).filter(
    (child): child is HTMLElement => child instanceof HTMLElement,
  );
}

export function masonryRowSpan(
  height: number,
  rowHeight: number,
  rowGap: number,
): number | null {
  if (
    !Number.isFinite(height) ||
    height <= 0 ||
    !Number.isFinite(rowHeight) ||
    rowHeight <= 0 ||
    !Number.isFinite(rowGap) ||
    rowGap < 0
  ) {
    return null;
  }

  const span = Math.ceil((height + rowGap) / (rowHeight + rowGap));
  return Number.isFinite(span) && span > 0 ? span : null;
}

export function resetMasonryGrid(grid: HTMLElement): void {
  grid.removeAttribute("data-masonry-ready");
  for (const item of directItems(grid)) {
    item.style.removeProperty("grid-row-end");
  }
}

export function measureMasonryGrid(grid: HTMLElement): boolean {
  try {
    const items = directItems(grid);
    if (items.length === 0) {
      resetMasonryGrid(grid);
      return false;
    }

    const computedStyle = getComputedStyle(grid);
    const rowHeight = Number.parseFloat(
      computedStyle.getPropertyValue("--masonry-row-size"),
    );
    const rowGap = Number.parseFloat(computedStyle.rowGap);
    const spans: number[] = [];

    for (const item of items) {
      const span = masonryRowSpan(
        item.getBoundingClientRect().height,
        rowHeight,
        rowGap,
      );
      if (span === null) {
        resetMasonryGrid(grid);
        return false;
      }
      spans.push(span);
    }

    for (const [index, item] of items.entries()) {
      const gridRowEnd = `span ${spans[index]}`;
      if (item.style.gridRowEnd !== gridRowEnd) {
        item.style.gridRowEnd = gridRowEnd;
      }
    }
    grid.dataset.masonryReady = "true";
    return true;
  } catch {
    resetMasonryGrid(grid);
    return false;
  }
}

export function useMasonryGrid<T extends HTMLElement>(
  itemCount: number,
): RefObject<T | null> {
  const gridRef = useRef<T | null>(null);

  useEffect(() => {
    const grid = gridRef.current;
    if (
      !grid ||
      typeof ResizeObserver !== "function" ||
      typeof requestAnimationFrame !== "function" ||
      typeof cancelAnimationFrame !== "function"
    ) {
      return;
    }

    let pendingFrame: number | null = null;
    const scheduleMeasurement = () => {
      if (pendingFrame !== null) {
        return;
      }
      pendingFrame = requestAnimationFrame(() => {
        pendingFrame = null;
        measureMasonryGrid(grid);
      });
    };
    const observer = new ResizeObserver(scheduleMeasurement);

    observer.observe(grid);
    for (const item of directItems(grid)) {
      observer.observe(item);
    }
    scheduleMeasurement();

    return () => {
      observer.disconnect();
      if (pendingFrame !== null) {
        cancelAnimationFrame(pendingFrame);
      }
      resetMasonryGrid(grid);
    };
  }, [itemCount]);

  return gridRef;
}
