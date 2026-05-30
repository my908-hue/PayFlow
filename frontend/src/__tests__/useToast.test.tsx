import React from "react";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { vi, describe, it, expect, beforeEach, afterEach } from "vitest";

describe("useToast hook", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("adds a toast and auto-dismisses after 5s", () => {
    const { default: ReactDefault } = require("react");
    const { useToast } = require("../hooks/useToast");

    function Test() {
      const { toasts, addToast } = useToast();
      return (
        <div>
          <button onClick={() => addToast("hello", "info")}>
            add
          </button>
          <div data-testid="list">
            {toasts.map((t: { id: string; message: string }) => (
              <div key={t.id} data-testid={`toast-${t.id}`}>
                {t.message}
              </div>
            ))}
          </div>
        </div>
      );
    }

    render(ReactDefault.createElement(Test));

    const btn = screen.getByText("add");
    fireEvent.click(btn);

    expect(screen.getByTestId(/toast-/)).toBeTruthy();

    act(() => {
      vi.advanceTimersByTime(5000);
    });

    expect(screen.queryByTestId(/toast-/)).toBeNull();
  });

  it("allows duplicate messages (unique ids)", () => {
    vi.resetModules();
    const { default: ReactDefault } = require("react");
    const { useToast } = require("../hooks/useToast");

    function Test() {
      const { toasts, addToast } = useToast();
      return (
        <div>
          <button onClick={() => addToast("dup", "info")}>
            add
          </button>
          <div data-testid="list">
            {toasts.map((t: { id: string; message: string }) => (
              <div key={t.id} data-testid={`toast-${t.id}`}>
                {t.message}
              </div>
            ))}
          </div>
        </div>
      );
    }

    render(ReactDefault.createElement(Test));
    const btn = screen.getByText("add");
    fireEvent.click(btn);
    fireEvent.click(btn);

    const items = screen.getAllByTestId(/toast-/);
    expect(items.length).toBe(2);
    expect(items[0].textContent).toBe("dup");
    expect(items[1].textContent).toBe("dup");
  });
});
