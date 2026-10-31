import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";

import type { Backend } from "./backend";
import type { Analysis, Applied, Listing, Track, TrackRow } from "./types";

const XML_FILTER = [{ name: "rekordbox XML", extensions: ["xml"] }];

export const tauriBackend: Backend = {
  loadRules: () => invoke<string>("load_rules"),
  defaultRules: () => invoke<string>("default_rules"),
  initialPaths: () => invoke<string[]>("initial_paths"),
  openLibrary: () => invoke<Listing>("open_rekordbox_library"),
  openXml: (path) => invoke<Listing>("open_rekordbox", { path }),
  loadPlaylist: (index) => invoke<TrackRow[]>("load_playlist", { index }),
  analyse: (path) => invoke<Analysis>("analyse", { path }),
  analyseRekordbox: (id) => invoke<Analysis>("analyse_rekordbox", { id }),
  trackDetail: (path) => invoke<Track>("track_detail", { path }),
  applyRules: (rules, save) => invoke<[string, Applied][]>("apply_rules", { rules, save }),
  applyRulesTo: (path, rules) => invoke<Applied>("apply_rules_to", { path, rules }),
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
