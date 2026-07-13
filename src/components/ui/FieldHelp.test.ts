import { describe, expect, it, vi } from "vitest";
import { announceFieldHelpOpen, subscribeToFieldHelpOpen } from "./FieldHelp";

describe("field help coordination", () => {
  it("notifies an existing popover when another popover opens", () => {
    const listener = vi.fn();
    const unsubscribe = subscribeToFieldHelpOpen(listener);

    announceFieldHelpOpen("field-help-release");

    expect(listener).toHaveBeenCalledWith("field-help-release");
    unsubscribe();
  });

  it("stops notifying a popover after it unsubscribes", () => {
    const listener = vi.fn();
    const unsubscribe = subscribeToFieldHelpOpen(listener);
    unsubscribe();

    announceFieldHelpOpen("field-help-attack");

    expect(listener).not.toHaveBeenCalled();
  });
});
