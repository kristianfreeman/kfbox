import { Component, type ErrorInfo, type ReactNode } from "react";

/** Errors the page hit, newest last: for the smoke test's report and the shell's log. */
export const pageErrors: string[] = [];

/** Keeps an error, and hands it to `report` (the shell's log in the app). */
export function noteError(error: unknown, report?: (text: string) => void) {
  const text = error instanceof Error ? `${error.message}\n${error.stack ?? ""}` : String(error);
  pageErrors.push(text);
  report?.(text);
}

/** Shows what went wrong, with a way back, instead of an empty window when the page fails to draw. */
export class Crash extends Component<{ children: ReactNode; report?: (text: string) => void }, { error: string | null }> {
  state = { error: null as string | null };

  static getDerivedStateFromError(error: unknown) {
    return { error: error instanceof Error ? error.message : String(error) };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    noteError(
      error instanceof Error ? Object.assign(error, { stack: `${error.stack ?? ""}${info.componentStack ?? ""}` }) : error,
      this.props.report,
    );
  }

  render() {
    if (this.state.error === null) return this.props.children;
    return (
      <div className="crash" role="alert">
        <h1>kfbox hit a problem drawing the window</h1>
        <p>{this.state.error}</p>
        <button className="quiet-accent" onClick={() => location.reload()}>
          Reload
        </button>
      </div>
    );
  }
}
