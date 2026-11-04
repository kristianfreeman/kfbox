// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Crash, pageErrors } from "./Crash";

afterEach(cleanup);

function Broken(): never {
  throw new Error("cannot draw this track");
}

describe("Crash", () => {
  it("shows what went wrong instead of an empty window, and reports it", () => {
    const report = vi.fn();
    const quiet = vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(
      <Crash report={report}>
        <Broken />
      </Crash>,
    );
    quiet.mockRestore();
    expect(screen.getByRole("alert").textContent).toContain("cannot draw this track");
    expect(screen.getByRole("button", { name: "Reload" })).toBeTruthy();
    expect(report).toHaveBeenCalledWith(expect.stringContaining("cannot draw this track"));
    expect(pageErrors.at(-1)).toContain("cannot draw this track");
  });
});
