import type { Store } from "../app/store";

const SEPARATOR = " › ";

export function Sidebar({ store }: { store: Store }) {
  return (
    <nav className="sidebar" aria-label="Library">
      <button className="source" aria-current={store.chosenPath === null && !store.reviewing} onClick={() => void store.choose(null)}>
        <span>All Tracks</span>
        {store.total !== null && <span className="count">{store.total}</span>}
      </button>
      <button className="source" aria-current={store.reviewing} onClick={() => void store.chooseReview()}>
        <span>Needs Review</span>
        <span className="count">{store.toReview.length}</span>
      </button>
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
            aria-current={store.chosenPath === playlist.path}
            onClick={() => void store.choose(playlist)}
          >
            <span>{name}</span>
            <span className="count">{playlist.count}</span>
          </button>
        );
      })}
    </nav>
  );
}
