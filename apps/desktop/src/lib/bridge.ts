import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { NoteSummary } from "@notedock/editor";
import type { AppInfo, LocalNote, SyncState, WindowPrefs } from "@notedock/editor/generated/desktop";

/** Must match `notedock_desktop_lib::sync::SYNC_EVENT`. */
const SYNC_EVENT = "notedock:sync";

/**
 * Every call the window can make into Rust.
 *
 * Argument keys are camelCase: Tauri converts them to the command's snake_case
 * parameters. There is no `fetch` here and no `save_note` — the note list comes
 * from Rust, and note bodies travel over their own WebSocket as Yjs updates. The
 * only credential that reaches this side is the short-lived URL from
 * {@link wsUrl}, never the bearer token.
 */
export const bridge = {
  listNotes: () => invoke<NoteSummary[]>("list_notes"),
  getNote: (id: string) => invoke<LocalNote | null>("get_note", { id }),
  createNote: (title: string) => invoke<LocalNote>("create_note", { title }),
  deleteNote: (id: string) => invoke<void>("delete_note", { id }),

  /** A fresh single-use document-socket URL. Spent on use, so call per connect. */
  wsUrl: () => invoke<string>("ws_url"),

  syncNow: () => invoke<SyncState>("sync_now"),
  syncState: () => invoke<SyncState>("sync_state"),
  getSpotlight: () => invoke<string | null>("get_spotlight"),
  setSpotlight: (id: string | null) => invoke<void>("set_spotlight", { id }),

  login: (serverUrl: string, password: string) =>
    invoke<SyncState>("login", { serverUrl, password }),
  logout: () => invoke<SyncState>("logout"),
  openWeb: () => invoke<void>("open_web"),

  setClickThrough: (active: boolean) => invoke<void>("set_click_through", { active }),
  setAlwaysOnTop: (onTop: boolean) => invoke<void>("set_always_on_top", { onTop }),
  windowPrefs: () => invoke<WindowPrefs>("window_prefs"),
  /** Persists the opacity. Call on slider release, not on every tick. */
  setOpacity: (value: number) => invoke<void>("set_opacity", { value }),
  appInfo: () => invoke<AppInfo>("app_info"),
  quit: () => invoke<void>("quit"),

  onSync: (handler: (state: SyncState) => void): Promise<UnlistenFn> =>
    listen<SyncState>(SYNC_EVENT, (event) => handler(event.payload)),
};

/** Tauri command errors arrive as plain strings. */
export function reason(error: unknown): string {
  return typeof error === "string" ? error : error instanceof Error ? error.message : "未知错误";
}
