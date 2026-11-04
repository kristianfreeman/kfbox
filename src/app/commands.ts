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

/** Commands that take what's typed as a name: a new playlist, or a playlist's new name. */
export function named(store: Store, text: string): Command[] {
  const name = text.trim();
  if (!name || !store.libraryOpen) return [];
  const entry = store.current;
  const editable = store.editablePlaylist;
  const list: Command[] = [
    {
      id: "new-playlist",
      label: entry?.id ? `New playlist “${name}” with ${entry.row.title}` : `New playlist “${name}”`,
      run: () => store.newPlaylist(name),
    },
  ];
  if (editable) list.push({ id: "rename-playlist", label: `Rename ${editable.path} to “${name}”`, run: () => store.renamePlaylist(name) });
  return list;
}

/** Everything the palette's Commands section offers, in the order shown. */
export function commands(store: Store): Command[] {
  const entry = store.current;
  const targets = store.ruleTargets.length;
  const editable = store.editablePlaylist;
  const title = entry?.row.title ?? "the selection";
  const playlistCommands: Command[] = entry?.id
    ? store.addablePlaylists
        .filter((p) => p.path !== editable?.path)
        .map((p) => ({ id: `add-to-${p.index}`, label: `Add ${title} to ${p.path}`, run: () => store.addToPlaylist(p) }))
    : [];
  if (editable && entry?.id) {
    playlistCommands.push({ id: "remove-from", label: `Remove ${title} from ${editable.path}`, run: () => store.removeFromPlaylist() });
  }
  if (editable)
    playlistCommands.push({ id: "delete-playlist", label: `Delete playlist ${editable.path}`, run: () => store.deletePlaylist() });
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
    {
      id: "add-songs",
      label: store.libraryOpen ? "Import songs into the library…" : "Add songs…",
      keys: "⇧⌘O",
      run: () => store.pickSongs(),
    },
    {
      id: "find-missing",
      label: `Find ${store.counts.missing} missing files in a folder…`,
      run: () => store.findMissing(),
      disabled: !store.libraryOpen || store.counts.missing === 0,
    },
    { id: "reload", label: "Reload library", run: () => store.openLibrary() },
    { id: "export", label: "Export XML…", keys: "⌘E", run: () => store.exportXml(), disabled: !store.canExport },
    { id: "rules", label: "Rules", run: () => store.chooseView("rules") },
    { id: "wizard", label: "Rules wizard…", run: () => store.openSheet("wizard") },
    { id: "history", label: "History", run: () => store.chooseView("history") },
    { id: "track-view", label: store.trackOpen ? "Hide track view" : "Show track view", keys: "T", run: () => store.toggleTrack() },
    { id: "settings", label: "Settings…", keys: "⌘,", run: () => store.openSheet("settings") },
    { id: "undo", label: "Undo", keys: "⌘Z", run: () => store.undo() },
    { id: "redo", label: "Redo", keys: "⇧⌘Z", run: () => store.redo() },
    ...playlistCommands,
  ];
}

/** Commands whose words all appear in the label, in any case. */
export function matching(list: Command[], text: string): Command[] {
  const words = text.toLowerCase().split(/\s+/).filter(Boolean);
  return list.filter((c) => words.every((w) => c.label.toLowerCase().includes(w)));
}
