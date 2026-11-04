import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";

import type { Backend } from "./backend";
import type {
  Analysis,
  Applied,
  AssistantSettings,
  AutoCueEvent,
  Drive,
  HistoryEntry,
  ImportReport,
  LibraryChange,
  Listing,
  PlayerTick,
  QuickResult,
  Saved,
  Score,
  Track,
  TrackRow,
  UsbProgress,
  UsbReport,
  WizardTurn,
} from "./types";

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
  waveform: (path) => invoke<number[]>("waveform", { path }),
  playerLoad: (path) => invoke<number>("player_load", { path }),
  playerPlay: () => invoke<void>("player_play"),
  playerPause: () => invoke<void>("player_pause"),
  playerSeek: (secs) => invoke<void>("player_seek", { secs }),
  playerSetLoop: (from, to) => invoke<void>("player_set_loop", { from, to }),
  playerClearLoop: () => invoke<void>("player_clear_loop"),
  playerScrubBegin: () => invoke<void>("player_scrub_begin"),
  playerScrubTo: (secs) => invoke<void>("player_scrub_to", { secs }),
  playerScrubEnd: () => invoke<void>("player_scrub_end"),
  onPlayerTick: (listener) => {
    void listen<PlayerTick>("player-tick", (event) => listener(event.payload));
  },
  onPlayerError: (listener) => {
    void listen<{ load_id: number; message: string }>("player-error", (event) => listener(event.payload));
  },
  applyRulesTo: (path, rules) => invoke<Applied>("apply_rules_to", { path, rules }),
  exportXml: (dest, rules) => invoke<number>("export_xml", { dest, rules }),
  saveCues: (summary, edits) => invoke<Saved>("save_cues", { summary, edits }),
  writeRuleCues: (ids, rules) => invoke<Saved>("write_rule_cues", { ids, rules }),
  importFiles: (paths) => invoke<ImportReport>("import_files", { paths }),
  relocateMissing: (folder) => invoke<Saved>("relocate_missing", { folder }),
  setRating: (ids, stars) => invoke<Saved>("set_rating", { ids, stars }),
  setMyTags: (ids, tags, mode) => invoke<Saved>("set_my_tags", { ids, tags, mode }),
  createPlaylist: (name, folder, ids) => invoke<Saved>("create_playlist", { name, folder, ids }),
  addToPlaylist: (index, ids) => invoke<Saved>("add_to_playlist", { index, ids }),
  removeFromPlaylist: (index, ids) => invoke<Saved>("remove_from_playlist", { index, ids }),
  renamePlaylist: (index, name) => invoke<Saved>("rename_playlist", { index, name }),
  deletePlaylist: (index) => invoke<Saved>("delete_playlist", { index }),
  editGrid: (id, edit) => invoke<Saved>("edit_grid", { id, edit }),
  autoCueEnabled: () => invoke<boolean>("auto_cue_enabled"),
  setAutoCue: (on) => invoke<boolean>("set_auto_cue", { on }),
  onAutoCue: (listener) => {
    void listen<AutoCueEvent>("auto-cue", (event) => listener(event.payload));
  },
  usbDrives: () => invoke<Drive[]>("usb_drives"),
  exportUsb: (destination, playlists, fileSystem) => invoke<UsbReport>("export_usb", { destination, playlists, fileSystem }),
  cancelUsb: () => invoke("cancel_usb"),
  onUsbProgress: (listener) => {
    void listen<UsbProgress>("usb-progress", (event) => listener(event.payload));
  },
  undo: () => invoke<Saved>("undo"),
  redo: () => invoke<Saved>("redo"),
  history: (limit) => invoke<HistoryEntry[]>("history", { limit }),
  restore: (commit) => invoke<Saved>("restore", { commit }),

  assistantSettings: () => invoke<AssistantSettings>("assistant_settings"),
  setAssistantModel: (model) => invoke<AssistantSettings>("set_assistant_model", { model }),
  saveApiKey: (key) => invoke<AssistantSettings>("save_api_key", { key }),
  removeApiKey: () => invoke<AssistantSettings>("remove_api_key"),
  runQuick: (instruction, track, playlist) => invoke<QuickResult>("run_quick", { instruction, track, playlist }),
  wizardTurn: (said) => invoke<WizardTurn>("wizard_turn", { said }),
  wizardReset: () => invoke<void>("wizard_reset"),
  hasSavedRules: () => invoke<boolean>("has_saved_rules"),
  saveRulesKeepingPrevious: (rules) => invoke<void>("save_rules_keeping_previous", { rules }),
  restorePreviousRules: () => invoke<string>("restore_previous_rules"),
  scoreRules: (rules) => invoke<Score>("score_rules", { rules }),
  pickAudio: async (extensions) => {
    const picked = await open({ multiple: true, filters: [{ name: "Audio", extensions }] });
    return picked ?? [];
  },
  pickXml: async () => {
    const picked = await open({ filters: XML_FILTER });
    return typeof picked === "string" ? picked : null;
  },
  pickExportPath: (defaultPath) => save({ defaultPath, filters: XML_FILTER }),
  pickFolder: async () => {
    const picked = await open({ directory: true });
    return typeof picked === "string" ? picked : null;
  },
  nativeMenu: true,
  onMenu: (listener) => {
    void listen<string>("menu", (event) => listener(event.payload));
  },
  onLibraryChanged: (listener) => {
    void listen<LibraryChange>("library-changed", (event) => listener(event.payload));
  },
  onHistoryChanged: (listener) => {
    void listen("history-changed", () => listener());
  },
  onFileDrop: (listener) => {
    void getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "drop") listener(event.payload.paths);
    });
  },
};
