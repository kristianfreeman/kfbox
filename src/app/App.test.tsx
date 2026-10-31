// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { mockBackend } from "../ipc/mock";
import { App } from "./App";
import { Store } from "./store";

afterEach(cleanup);

describe("App", () => {
  it("lists tracks, switches playlist, and opens the rules", async () => {
    const store = new Store(mockBackend(0));
    render(<App store={store} />);
    await store.start();
    await screen.findByRole("button", { name: /Tideline/ });
    await waitFor(() => expect(store.entries.every((e) => !e.busy)).toBe(true));

    fireEvent.click(screen.getByRole("button", { name: /Warm up/ }));
    await screen.findByRole("button", { name: /Salt Flats/ });
    expect(screen.queryByRole("button", { name: /Tideline/ })).toBeNull();

    expect(screen.queryByLabelText("Rules file")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Rules" }));
    expect((screen.getByLabelText("Rules file") as HTMLTextAreaElement).value).toContain("[[cue]]");
  });
});
