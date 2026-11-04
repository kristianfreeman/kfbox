import { ChevronDown, Search, Usb } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { HistoryView } from "../views/HistoryView";
import { NowBar } from "../views/NowBar";
import { Palette } from "../views/Palette";
import { RulesView } from "../views/RulesView";
import { ImportSheet, SettingsSheet, UsbSheet, WizardSheet } from "../views/Sheets";
import { Sidebar } from "../views/Sidebar";
import { TrackDock } from "../views/TrackDock";
import { TrackTable } from "../views/TrackTable";
import { Mark } from "../views/Mark";
import { useStore, type Store } from "./store";

/** Rows PageUp and PageDown move by. */
const PAGE_ROWS = 12;

/** The header's words: the view's title and its count, or what kfbox is busy with. Problems show here;
 * saving doesn't, since it's quick and normal. */
function Heading({ store }: { store: Store }) {
  const activity = store.activity;
  const count =
    store.view === "tracks"
      ? store.query
        ? `${store.rows.length} of ${store.entries.length}`
        : String(store.rows.length)
      : store.view === "history"
        ? String(store.history.length || "")
        : "";
  return (
    <div className="heading" role="status" data-tauri-drag-region>
      {activity ? (
        <span className={activity.waiting ? "activity waiting" : "activity"} data-tauri-drag-region>
          {activity.text}
        </span>
      ) : (
        <>
          <h1 data-tauri-drag-region>{store.title}</h1>
          {count && (
            <span className="count" data-tauri-drag-region>
              {count}
            </span>
          )}
        </>
      )}
      {store.writeError ? (
        <span className="problem" title={store.writeError}>
          {store.writeError}
        </span>
      ) : (
        store.status && (
          <span className="notice" title={store.status}>
            {store.status}
          </span>
        )
      )}
    </div>
  );
}

/** The header's Export: a hairline button that names the plugged-in stick, with a dot and a count when
 * the stick is behind the library. Its menu: a stick to update, a USB export, or rekordbox XML. */
function ExportMenu({ store }: { store: Store }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (event: Event) => {
      if (!box.current?.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);
  const run = (action: () => unknown) => () => {
    setOpen(false);
    void action();
  };
  const stick = store.libraryOpen ? (store.drives.find((d) => d.stick) ?? store.drives[0]) : undefined;
  const behind = stick ? (store.behind[stick.path] ?? 0) : 0;
  return (
    <div className="export-menu" ref={box}>
      <button className="export" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span className="label">{stick ? `Update ${stick.name}` : "Export"}</span>
        {behind > 0 && (
          <span className="behind" title={`${behind} ${behind === 1 ? "track" : "tracks"} to update on ${stick?.name}`}>
            <span className="dot" aria-hidden="true" />
            {behind}
          </span>
        )}
        <ChevronDown size={14} strokeWidth={1.75} aria-hidden="true" />
      </button>
      {open && (
        <div className="menu" role="menu" aria-label="Export">
          {store.libraryOpen &&
            store.drives.map((drive) => (
              <button key={drive.path} role="menuitem" onClick={run(() => store.openUsb(drive))}>
                <Usb size={14} strokeWidth={1.75} aria-hidden="true" />
                {drive.stick && !drive.stick.ours ? `Replace ${drive.name} with kfbox…` : `Update ${drive.name}`}
              </button>
            ))}
          <button role="menuitem" disabled={!store.libraryOpen} onClick={run(() => store.openUsb())}>
            <span className="icon-space" />
            Export to USB…
            <kbd>⇧⌘E</kbd>
          </button>
          <button role="menuitem" disabled={!store.canExport} onClick={run(() => store.exportXml())}>
            <span className="icon-space" />
            rekordbox XML…
            <kbd>⌘E</kbd>
          </button>
        </div>
      )}
    </div>
  );
}

export function App({ store: source }: { store: Store }) {
  const store = useStore(source);
  const search = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target?.tagName === "INPUT" || target?.tagName === "TEXTAREA";
      const command = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();
      if (command && key === "k") {
        event.preventDefault();
        if (source.quickOpen) source.closeQuick();
        else source.openQuick();
        return;
      }
      if (source.quickOpen || source.sheet) {
        if (event.key === "Escape") {
          if (source.quickOpen) source.closeQuick();
          else source.openSheet(null);
        }
        return;
      }
      // With a menu bar, its items own their shortcuts and arrive as menu events.
      if (command && !source.backend.nativeMenu) {
        const menu: Record<string, () => unknown> = {
          o: () => (event.shiftKey ? source.pickSongs() : source.pickXml()),
          e: () => (event.shiftKey ? source.openUsb() : source.exportXml()),
          ",": () => source.openSheet("settings"),
          "1": () => source.chooseView("tracks"),
          "2": () => source.chooseView("rules"),
          "3": () => source.chooseView("history"),
        };
        if (!typing) menu.z = () => (event.shiftKey ? source.redo() : source.undo());
        const action = menu[key];
        if (action) {
          event.preventDefault();
          void action();
          return;
        }
      }
      if (command && key === "r") {
        event.preventDefault();
        void source.applyRulesToSelected();
        return;
      }
      if (event.key === "Escape") {
        if (target === search.current) {
          source.setQuery("");
          search.current?.blur();
        } else if (source.trackOpen) {
          source.closeTrack();
        }
        return;
      }
      // Up, Down and Enter also work from the search box, to pick a result.
      const fromSearch = target === search.current;
      if (typing && !fromSearch) return;
      if (command || event.altKey) return;
      const { player } = source;
      const actions: Record<string, () => void> = {
        ArrowUp: () => source.moveSelection(-1),
        ArrowDown: () => source.moveSelection(1),
        Enter: () => {
          source.openTrack();
          source.playSelected();
        },
      };
      if (!typing) {
        // A to H jump to hot cues; Shift-A to Shift-H set them at the playhead.
        if (/^[a-h]$/.test(event.key)) actions[event.key] = () => void player.jumpToCue(event.key);
        if (/^[A-H]$/.test(event.key) && event.shiftKey) actions[event.key] = () => void source.setCueAtPlayhead(event.key);
        // 0 to 5 rate the selected track.
        if (/^[0-5]$/.test(event.key)) actions[event.key] = () => source.rate(Number(event.key));
        Object.assign(actions, {
          " ": () => source.togglePlay(),
          ArrowLeft: () => void player.seekBy(event.shiftKey ? -4 : -1),
          ArrowRight: () => void player.seekBy(event.shiftKey ? 4 : 1),
          l: () => void player.toggleLoop(),
          q: () => source.enqueueSelected(),
          t: () => source.toggleTrack(),
          "]": () => void player.next(),
          m: () => void source.setCueAtPlayhead("memory"),
          Delete: () => void source.deleteSelectedCue(),
          Backspace: () => void source.deleteSelectedCue(),
          "+": () => source.zoomBy(2),
          "=": () => source.zoomBy(2),
          "-": () => source.zoomBy(0.5),
          PageUp: () => source.moveSelection(-PAGE_ROWS),
          PageDown: () => source.moveSelection(PAGE_ROWS),
          Home: () => source.selectEdge(false),
          End: () => source.selectEdge(true),
          "/": () => search.current?.focus(),
        });
      }
      const action = actions[event.key];
      // A focused button handles its own Enter. Space always plays, so the last
      // button clicked doesn't get pressed again.
      if (!action || (event.key === "Enter" && target?.tagName === "BUTTON")) return;
      event.preventDefault();
      action();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [source]);

  return (
    <div className={store.backend.nativeMenu ? "app native" : "app"}>
      <header className="top" data-tauri-drag-region>
        <div className="top-mark" data-tauri-drag-region>
          <Mark store={store} />
        </div>
        <Heading store={store} />
        <div className="search">
          <Search size={14} strokeWidth={1.75} aria-hidden="true" />
          <input
            ref={search}
            type="search"
            placeholder="Search"
            aria-label="Search tracks"
            value={store.query}
            onChange={(event) => store.setQuery(event.target.value)}
          />
          <button className="hint" aria-label="Command palette" onClick={() => store.openQuick()}>
            <kbd>⌘K</kbd>
          </button>
        </div>
        <ExportMenu store={store} />
      </header>
      <Sidebar store={store} />
      <main className="main">
        {store.view === "rules" && <RulesView store={store} />}
        {store.view === "history" && <HistoryView store={store} />}
        {store.view === "tracks" && (
          <>
            <TrackTable store={store} />
            {store.trackOpen && <TrackDock store={store} />}
          </>
        )}
      </main>
      <NowBar store={store} />
      {store.quickOpen && <Palette store={store} />}
      {store.sheet === "settings" && <SettingsSheet store={store} />}
      {store.sheet === "wizard" && <WizardSheet store={store} />}
      {store.sheet === "import" && <ImportSheet store={store} />}
      {store.sheet === "usb" && <UsbSheet store={store} />}
    </div>
  );
}
