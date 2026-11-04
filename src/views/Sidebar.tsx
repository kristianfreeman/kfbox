import type { Smart, Store } from "../app/store";

const SEPARATOR = " › ";

const LISTS: { id: Smart; label: string }[] = [
  { id: "all", label: "All tracks" },
  { id: "recent", label: "Recently added" },
  { id: "nocues", label: "No cues" },
];

export function Sidebar({ store }: { store: Store }) {
  const onTracks = store.view === "tracks";
  return (
    <nav className="sidebar" aria-label="Library">
      {LISTS.map((list) => (
        <button
          key={list.id}
          className="source"
          aria-current={onTracks && store.chosenPath === null && store.smart === list.id}
          onClick={() => void store.chooseSmart(list.id)}
        >
          <span>{list.label}</span>
          <span className="count">{store.counts[list.id]}</span>
        </button>
      ))}
      {store.playlists.length > 0 && <h2>Playlists</h2>}
      {store.playlists.map((playlist) => {
        const parts = playlist.path.split(SEPARATOR);
        const indent = { paddingLeft: `${12 + (parts.length - 1) * 14}px` };
        const name = parts[parts.length - 1];
        return playlist.folder ? (
          <div className="folder" style={indent} key={playlist.path}>
            {name}
          </div>
        ) : (
          <button
            className="source"
            style={indent}
            key={playlist.path}
            aria-current={onTracks && store.chosenPath === playlist.path}
            onClick={() => void store.choose(playlist)}
          >
            <span>{name}</span>
            <span className="count">{playlist.count}</span>
          </button>
        );
      })}
      <h2>Prep</h2>
      <button className="source" aria-current={store.view === "rules"} onClick={() => void store.chooseView("rules")}>
        <span>Rules</span>
      </button>
      <button className="source" aria-current={store.view === "history"} onClick={() => void store.chooseView("history")}>
        <span>History</span>
      </button>
    </nav>
  );
}
