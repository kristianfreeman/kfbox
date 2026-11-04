import type { Store } from "./store";

/** One action from the palette, the keys or the menu bar. */
export interface Command {
  id: string;
  label: string;
  /** The shortcut, as shown. */
  keys?: string;
  run: () => unknown;
  disabled?: boolean;
}

/** Everything the palette's Commands section offers, in the order shown. */
export function commands(store: Store): Command[] {
  const entry = store.current;
  const targets = store.ruleTargets.length;
  return [
    {
      id: "apply-selection",
      label: entry ? `Apply rules to ${entry.row.title}` : "Apply rules to selection",
      keys: "⌘R",
      run: () => store.applyRulesToSelected(),
      disabled: !entry?.id,
    },
    {
      id: "apply-list",
      label: `Apply rules to all ${targets} with empty slots`,
      run: () => store.applyRulesToList(),
      disabled: targets === 0,
    },
    { id: "open-xml", label: "Open rekordbox XML…", keys: "⌘O", run: () => store.pickXml() },
    { id: "add-songs", label: "Add songs…", keys: "⇧⌘O", run: () => store.pickSongs() },
    { id: "reload", label: "Reload library", run: () => store.openLibrary() },
    { id: "export", label: "Export XML…", keys: "⌘E", run: () => store.exportXml(), disabled: !store.canExport },
    { id: "rules", label: "Rules", run: () => store.chooseView("rules") },
    { id: "wizard", label: "Rules wizard…", run: () => store.openSheet("wizard") },
    { id: "history", label: "History", run: () => store.chooseView("history") },
    { id: "track-view", label: store.trackOpen ? "Hide track view" : "Show track view", keys: "T", run: () => store.toggleTrack() },
    { id: "settings", label: "Settings…", keys: "⌘,", run: () => store.openSheet("settings") },
    { id: "undo", label: "Undo", keys: "⌘Z", run: () => store.undo() },
    { id: "redo", label: "Redo", keys: "⇧⌘Z", run: () => store.redo() },
  ];
}

/** Commands whose words all appear in the label, in any case. */
export function matching(list: Command[], text: string): Command[] {
  const words = text.toLowerCase().split(/\s+/).filter(Boolean);
  return list.filter((c) => words.every((w) => c.label.toLowerCase().includes(w)));
}
