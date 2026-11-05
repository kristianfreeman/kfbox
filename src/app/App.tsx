import { ChevronDown, PanelLeft, Search, Usb } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { HistoryView } from "../views/HistoryView";
import { Deck } from "../views/Deck";
import { Palette } from "../views/Palette";
import { RulesView } from "../views/RulesView";
import { ImportSheet, SettingsSheet, UsbSheet, WizardSheet } from "../views/Sheets";
import { Sidebar } from "../views/Sidebar";
import { TrackTable } from "../views/TrackTable";
import { Mark } from "../views/Mark";
import { useStore, type Store } from "./store";

/** Rows PageUp and PageDown move by. */
const PAGE_ROWS = 12;

/** What the activity view says: its tone, its words, and the moment it belongs to. Within one moment the words
 * change in place (work counting up); a new moment (play, pause, another track, work starting) cross-fades. */
interface Said {
  moment: string;
  tone: string;
  text: string;
  detail: string;
}

function said(store: Store): Said {
  const { activity, player } = store;
  const loaded = player.entry && player.key === player.entry.key ? player.entry : undefined;
  if (activity) return { moment: "activity", tone: activity.waiting ? "waiting" : "working", text: activity.text, detail: "" };
  if (store.writeError) return { moment: "problem", tone: "problem", text: store.writeError, detail: "" };
  if (store.status) return { moment: `notice ${store.status}`, tone: "notice", text: store.status, detail: "" };
  if (loaded && player.playing)
    return { moment: `playing ${loaded.key}`, tone: "playing", text: loaded.row.title, detail: loaded.row.artist };
  if (loaded) return { moment: `paused ${loaded.key}`, tone: "paused", text: loaded.row.title, detail: "Paused" };
  return { moment: "ready", tone: "idle", text: "Ready", detail: "" };
}

/** Longer than the words' fade out in app.css, after which the old words go whatever happens. */
const FADE_MS = 300;

/** The words now and, while they fade out, the words before. */
function useCrossfade(now: Said): { shown: Said; leaving: Said | null; gone: () => void } {
  const [state, setState] = useState<{ shown: Said; leaving: Said | null }>({ shown: now, leaving: null });
  let current = state;
  if (state.shown.moment !== now.moment) {
    current = { shown: now, leaving: state.shown };
    setState(current);
  }
  const leaving = current.leaving;
  const gone = useCallback(() => setState((s) => ({ ...s, leaving: null })), []);
  // The fade's end lets the old words go; if it never comes (no animations, a hidden window), so does a timer.
  useEffect(() => {
    if (!leaving) return;
    const timer = setTimeout(gone, FADE_MS);
    return () => clearTimeout(timer);
  }, [leaving, gone]);
  // Same moment: the newest words, without a fade.
  return { shown: { ...now }, leaving, gone };
}

/** The words now are the status a screen reader hears; the words fading out are hidden from it. */
function Words({ said: { text, detail }, className, onGone }: { said: Said; className: string; onGone?: () => void }) {
  return (
    <span
      className={className}
      data-tauri-drag-region
      role={onGone ? undefined : "status"}
      onAnimationEnd={onGone}
      aria-hidden={onGone ? true : undefined}
    >
      {text}
      {detail && <span className="activity-detail"> · {detail}</span>}
    </span>
  );
}

/** The activity view, centred in the toolbar as Xcode's is: the live mark, then what kfbox is doing (work, waiting
 * for rekordbox, a problem, a notice, or the track loaded), with a thin progress line while work runs. Saving isn't
 * announced: it's quick and normal. With a track
 * loaded, a click brings it back: selected, in view and in the deck, still playing. */
function ActivityView({ store }: { store: Store }) {
  const { activity, player } = store;
  const now = said(store);
  const { shown, leaving, gone } = useCrossfade(now);
  const progress = activity && !activity.waiting ? activity.progress : undefined;
  const loaded = !!player.entry && player.key === player.entry.key;
  const title = shown.detail ? `${shown.text} · ${shown.detail}` : shown.text;
  return (
    <div className="activity" data-tauri-drag-region>
      <div
        className={`activity-view ${now.tone}${loaded ? " focusable" : ""}`}
        title={loaded ? `${title} · Click to show it` : title}
        data-tauri-drag-region={loaded ? undefined : true}
        onClick={loaded ? () => void store.focusPlaying() : undefined}
      >
        <Mark store={store} />
        <span className="activity-words" data-tauri-drag-region={loaded ? undefined : true}>
          {leaving && <Words key={`out ${leaving.moment}`} said={leaving} className="activity-text leaving" onGone={gone} />}
          <Words key={`in ${shown.moment}`} said={shown} className={leaving ? "activity-text entering" : "activity-text"} />
        </span>
        {progress !== undefined && (
          <span className={progress === null ? "activity-progress busy" : "activity-progress"} aria-hidden="true">
            <span style={progress === null ? undefined : { width: `${Math.round(progress * 100)}%` }} />
          </span>
        )}
      </div>
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

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target?.tagName === "INPUT" || target?.tagName === "TEXTAREA";
      const command = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();
      if (event.ctrlKey && event.metaKey && key === "s" && !source.backend.nativeMenu) {
        event.preventDefault();
        source.toggleSidebar();
        return;
      }
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
        if (source.trackOpen) source.closeTrack();
        return;
      }
      if (typing) return;
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
        "/": () => source.openQuick(),
      });
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
    <div className={["app", store.backend.nativeMenu ? "native" : "", store.sidebarOpen ? "" : "no-sidebar"].filter(Boolean).join(" ")}>
      <header className="top" data-tauri-drag-region>
        <div className="top-left" data-tauri-drag-region>
          <div className="top-lead" data-tauri-drag-region>
            <button
              className="icon-button sidebar-toggle"
              aria-label={store.sidebarOpen ? "Hide sidebar" : "Show sidebar"}
              aria-pressed={store.sidebarOpen}
              title={`${store.sidebarOpen ? "Hide" : "Show"} Sidebar (⌃⌘S)`}
              onClick={() => store.toggleSidebar()}
            >
              <PanelLeft size={16} strokeWidth={1.75} aria-hidden="true" />
            </button>
          </div>
        </div>
        <ActivityView store={store} />
        <div className="top-trail">
          <button className="icon-button search-button" aria-label="Search" title="Search (⌘K)" onClick={() => store.openQuick()}>
            <Search size={16} strokeWidth={1.75} aria-hidden="true" />
          </button>
          <ExportMenu store={store} />
        </div>
      </header>
      <Sidebar store={store} away={!store.sidebarOpen} />
      <main className="main">
        {store.view === "rules" && <RulesView store={store} />}
        {store.view === "history" && <HistoryView store={store} />}
        {store.view === "tracks" && (
          <>
            <TrackTable store={store} />
            <Deck store={store} />
          </>
        )}
      </main>
      {store.quickOpen && <Palette store={store} />}
      {store.sheet === "settings" && <SettingsSheet store={store} />}
      {store.sheet === "wizard" && <WizardSheet store={store} />}
      {store.sheet === "import" && <ImportSheet store={store} />}
      {store.sheet === "usb" && <UsbSheet store={store} />}
    </div>
  );
}
