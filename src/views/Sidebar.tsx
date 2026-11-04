import {
  ChevronDown,
  ChevronRight,
  CircleDashed,
  Clock,
  FileQuestionMark,
  Folder,
  ListFilter,
  ListMusic,
  Music,
  TriangleAlert,
  Usb,
  type LucideIcon,
} from "lucide-react";

import { SMART_LABELS, type Smart, type Store } from "../app/store";
import type { PlaylistItem } from "../ipc/types";
import { stickLine } from "./Sheets";

const SEPARATOR = " › ";

/** The library's own lists. All tracks and Recently added are always there; the rest only when they hold something. */
const LISTS: { id: Smart; icon: LucideIcon; always?: boolean; warn?: boolean }[] = [
  { id: "all", icon: Music, always: true },
  { id: "recent", icon: Clock, always: true },
  { id: "nocues", icon: CircleDashed },
  { id: "early", icon: TriangleAlert, warn: true },
  { id: "missing", icon: FileQuestionMark },
];

const ICON = { size: 15, strokeWidth: 1.75, "aria-hidden": true } as const;

/** Playlists inside a folded folder are hidden. */
function hidden(playlist: PlaylistItem, collapsed: string[]): boolean {
  return collapsed.some((folder) => playlist.path.startsWith(folder + SEPARATOR));
}

export function Sidebar({ store }: { store: Store }) {
  const onTracks = store.view === "tracks";
  const lists = LISTS.filter((list) => list.always || store.counts[list.id] > 0);
  return (
    <nav className="sidebar" aria-label="Library">
      <h2>Library</h2>
      {lists.map(({ id, icon: Icon, warn }) => (
        <button
          key={id}
          className={warn ? "source warn" : "source"}
          aria-current={onTracks && store.chosenPath === null && store.smart === id}
          onClick={() => void store.chooseSmart(id)}
        >
          <Icon {...ICON} />
          <span className="name">{SMART_LABELS[id]}</span>
          <span className="count">{store.counts[id]}</span>
        </button>
      ))}
      {store.playlists.length > 0 && <h2>Playlists</h2>}
      {store.playlists
        .filter((playlist) => !hidden(playlist, store.collapsed))
        .map((playlist) => {
          const parts = playlist.path.split(SEPARATOR);
          const indent = { paddingLeft: `${8 + (parts.length - 1) * 14}px` };
          const name = parts[parts.length - 1];
          if (playlist.folder) {
            const shut = store.collapsed.includes(playlist.path);
            const Chevron = shut ? ChevronRight : ChevronDown;
            return (
              <button
                className="source folder"
                style={indent}
                key={playlist.path}
                aria-expanded={!shut}
                onClick={() => store.toggleFolder(playlist.path)}
              >
                <Chevron size={12} strokeWidth={2} aria-hidden="true" className="chevron" />
                <Folder {...ICON} />
                <span className="name">{name}</span>
              </button>
            );
          }
          const Icon = playlist.smart ? ListFilter : ListMusic;
          return (
            <button
              className="source"
              style={indent}
              key={playlist.path}
              aria-current={onTracks && store.chosenPath === playlist.path}
              onClick={() => void store.choose(playlist)}
            >
              <span className="chevron-space" />
              <Icon {...ICON} />
              <span className="name">{name}</span>
              <span className="count">{playlist.count}</span>
            </button>
          );
        })}
      {store.drives.length > 0 && <h2>Devices</h2>}
      {store.drives.map((drive) => {
        const behind = store.behind[drive.path] ?? 0;
        return (
          <button
            key={drive.path}
            className="source device"
            title={`${drive.path} · ${stickLine(drive)}`}
            onClick={() => void store.openUsb(drive)}
          >
            <Usb {...ICON} />
            <span className="name">
              {drive.name}
              <span className="device-line">
                {stickLine(drive)}
                {drive.total_bytes ? ` · ${(drive.free_bytes / 1e9).toFixed(0)} GB free` : ""}
              </span>
            </span>
            {behind > 0 && (
              <span className="count behind" title={`${behind} to update`}>
                <span className="dot" aria-hidden="true" />
                {behind}
              </span>
            )}
          </button>
        );
      })}
    </nav>
  );
}
