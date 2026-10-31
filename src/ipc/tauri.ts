import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";

import type { Backend } from "./backend";
import type { Applied, Listing, PlaylistTrack, Track } from "./types";

const XML_FILTER = [{ name: "rekordbox XML", extensions: ["xml"] }];

export const tauriBackend: Backend = {
  loadRules: () => invoke<string>("load_rules"),
  defaultRules: () => invoke<string>("default_rules"),
  initialPaths: () => invoke<string[]>("initial_paths"),
  openLibrary: () => invoke<Listing>("open_rekordbox_library"),
  openXml: (path) => invoke<Listing>("open_rekordbox", { path }),
  loadPlaylist: (index) => invoke<PlaylistTrack[]>("load_playlist", { index }),
  analyse: (path) => invoke<Track>("analyse", { path }),
  analyseRekordbox: (id) => invoke<Track>("analyse_rekordbox", { id }),
  applyRules: (rules, save) => invoke<[string, Applied][]>("apply_rules", { rules, save }),
  exportXml: (dest, rules) => invoke<number>("export_xml", { dest, rules }),
  pickAudio: async (extensions) => {
    const picked = await open({ multiple: true, filters: [{ name: "Audio", extensions }] });
    return picked ?? [];
  },
  pickXml: async () => {
    const picked = await open({ filters: XML_FILTER });
    return typeof picked === "string" ? picked : null;
  },
  pickExportPath: (defaultPath) => save({ defaultPath, filters: XML_FILTER }),
  onFileDrop: (listener) => {
    void getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "drop") listener(event.payload.paths);
    });
  },
};
