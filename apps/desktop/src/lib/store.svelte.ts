import {
  exportAll,
  NoteSession,
  type ConnectionState,
  type NoteSummary,
} from "@notedock/editor";
import type { SyncState } from "@notedock/editor/generated/desktop";
import { bridge, reason } from "./bridge";

const OFFLINE: SyncState = {
  status: "offline",
  message: null,
  pending: 0,
  logged_in: false,
  server_url: "",
};

/**
 * All window state.
 *
 * There is no autosave and no document JSON here. The open note is a
 * {@link NoteSession} whose Y.Doc converges over its own WebSocket, so every
 * keystroke is already on its way out — "saving" is not an operation this app
 * performs. What Rust still owns is the note *list*, which is what makes the
 * window usable with no network.
 */
export class DesktopStore {
  notes = $state<NoteSummary[]>([]);
  selectedId = $state<string | null>(null);
  /**
   * The note to reopen on the next launch. Written automatically whenever a note
   * is opened, so the window always comes back to whatever was last being
   * written — there is nothing here for the user to decide or maintain.
   */
  reopenId = $state<string | null>(null);

  /** The open note's document. Replacing it is what makes the editor rebind. */
  session = $state<NoteSession | null>(null);
  title = $state("");

  /** Metadata sync, reported by Rust. */
  sync = $state<SyncState>(OFFLINE);
  /** The document socket, which is what the sync dot actually reflects. */
  connection = $state<ConnectionState>("offline");
  error = $state<string | null>(null);
  /** Transient confirmation, e.g. where an export landed. Clears itself. */
  notice = $state<string | null>(null);
  #noticeTimer: ReturnType<typeof setTimeout> | undefined;
  /**
   * True while an export is walking the note list. Exposed because that walk
   * waits on one socket per note, which is long enough that the button has to
   * stop accepting a second click.
   */
  exporting = $state(false);

  opacity = $state(1);
  alwaysOnTop = $state(true);
  /**
   * Whether the window is filling the screen. Deliberately not persisted: a
   * notepad you summon should come back the size you can see past.
   */
  maximized = $state(false);
  /** Not persisted on purpose: a window that starts click-through is a trap. */
  clickThrough = $state(false);
  /**
   * Whether Windows launches the app at sign-in. Not one of the window prefs
   * because it does not live in `settings.json` — the registry owns it, and the
   * user can change it from 任务管理器 → 启动 without this program running.
   */
  autostart = $state(false);
  info = $state<Awaited<ReturnType<typeof bridge.appInfo>> | null>(null);

  /**
   * What the dot shows.
   *
   * With a note open this follows the document socket — that is the connection the
   * user's keystrokes travel over. With no note open there is no socket, so it
   * falls back to the metadata loop's own state.
   */
  get status(): SyncState["status"] {
    if (!this.session) return this.sync.status;
    return this.connection === "live"
      ? "synced"
      : this.connection === "connecting"
        ? "syncing"
        : "offline";
  }

  async init(): Promise<() => void> {
    const offSync = await bridge.onSync((state) => {
      this.sync = state;
      void this.#reconcile();
    });
    const offMaximized = await bridge.onMaximized((maximized) => {
      this.maximized = maximized;
    });

    try {
      this.sync = await bridge.syncState();
      this.reopenId = await bridge.getSpotlight();
      const prefs = await bridge.windowPrefs();
      this.opacity = prefs.opacity;
      this.alwaysOnTop = prefs.always_on_top;
      this.maximized = await bridge.isMaximized();
      await this.refresh();
    } catch (error) {
      this.error = reason(error);
    }

    return () => {
      offSync();
      offMaximized();
    };
  }

  async refresh(): Promise<void> {
    try {
      this.notes = await bridge.listNotes();
      if (!this.selectedId) {
        // Reopen the note this window was last on; otherwise the most recent
        // one. A floating notepad that opens to nothing has wasted the summon.
        const preferred =
          this.reopenId && this.notes.some((note) => note.id === this.reopenId)
            ? this.reopenId
            : this.notes[0]?.id;
        if (preferred) this.open(preferred);
      }
    } catch (error) {
      this.error = reason(error);
    }
  }

  /**
   * Reacts to a completed metadata sync: pick up new notes, and drop the open one
   * if it was deleted elsewhere.
   *
   * Note that the *body* needs nothing here — it arrives over the document socket
   * as it is typed, which is why there is no longer any "reload the editor"
   * branch to get wrong.
   */
  async #reconcile(): Promise<void> {
    await this.refresh();
    const id = this.selectedId;
    if (!id) return;

    const summary = this.notes.find((note) => note.id === id);
    if (!summary) {
      this.#close();
      const next = this.notes[0];
      if (next) this.open(next.id);
      return;
    }
    // An open note's title is driven by its live Yjs document. The metadata list
    // is eventually consistent (the server materializes it on a timer), so
    // copying the summary here can briefly replace a freshly typed title with
    // the previous, still-stale value.
    if (!this.session) this.title = summary.title;
  }

  open(id: string, seedTitle = ""): void {
    if (id === this.selectedId) return;
    this.#close();

    // By the time a palette-created note is opened it is already in the list with
    // the name it was given, so `seedTitle` is only the fallback for the window
    // where it is not. Either way the session decides whether the *document*
    // adopts it — there is deliberately one write path for the title.
    const title = this.notes.find((note) => note.id === id)?.title ?? seedTitle;

    const session = new NoteSession({
      noteId: id,
      // The webview caches the document so a note opens instantly and stays
      // editable with the network down.
      cacheKey: `notedock:${id}`,
      ticket: () => bridge.wsUrl(),
      onState: (state) => (this.connection = state),
      initialTitle: title,
      onTitle: (next) => (this.title = next),
    });

    this.selectedId = id;
    this.session = session;
    this.title = title;

    // Remembered here rather than behind a toggle: the note you were last in is
    // the one you want back, and asking the user to pin it is asking them to do
    // the program's bookkeeping.
    this.reopenId = id;
    void bridge.setSpotlight(id).catch((error) => (this.error = reason(error)));
  }

  renameTitle(title: string): void {
    this.title = title;
    this.session?.setTitle(title);
  }

  async openWeb(): Promise<void> {
    try {
      await bridge.openWeb();
    } catch (error) {
      this.error = reason(error);
    }
  }

  async create(title = ""): Promise<void> {
    try {
      const note = await bridge.createNote(title);
      await this.refresh();
      this.open(note.id, title);
    } catch (error) {
      this.error = reason(error);
    }
  }

  async remove(id: string): Promise<void> {
    try {
      await bridge.deleteNote(id);
      if (id === this.selectedId) this.#close();
      await this.refresh();
    } catch (error) {
      this.error = reason(error);
    }
  }

  /**
   * Writes every note out as Markdown.
   *
   * Bodies are Yjs documents, so this is not a query over the note list: each
   * note has to be brought up to date before it can be read. {@link exportAll}
   * does that one note at a time and reuses the open one, which is why this
   * reports progress — it waits on the network once per note.
   */
  async exportNotes(): Promise<void> {
    if (this.exporting || this.notes.length === 0) return;
    this.exporting = true;
    // A pending announce from a previous export would otherwise blank the
    // progress line partway through this one.
    clearTimeout(this.#noticeTimer);
    try {
      const files = await exportAll({
        notes: this.notes,
        live: this.session,
        ticket: () => bridge.wsUrl(),
        cacheKey: (id) => `notedock:${id}`,
        onProgress: (done, total) => {
          this.notice = `正在导出 ${done}/${total} 篇…`;
        },
      });
      const dir = await bridge.exportNotes(files);
      this.#announce(`已导出 ${files.length} 篇到 ${dir}`);
    } catch (error) {
      this.notice = null;
      this.error = reason(error);
    } finally {
      this.exporting = false;
    }
  }

  /** A banner that needs dismissing is one more thing to click. */
  #announce(text: string): void {
    this.notice = text;
    clearTimeout(this.#noticeTimer);
    this.#noticeTimer = setTimeout(() => (this.notice = null), 8000);
  }

  /** Live preview while the slider moves; nothing is written to disk yet. */
  previewOpacity(value: number): void {
    this.opacity = value;
  }

  /** Called when the slider is released. */
  async commitOpacity(value: number): Promise<void> {
    this.opacity = value;
    try {
      await bridge.setOpacity(value);
    } catch (error) {
      this.error = reason(error);
    }
  }

  async toggleAlwaysOnTop(): Promise<void> {
    const next = !this.alwaysOnTop;
    try {
      await bridge.setAlwaysOnTop(next);
      this.alwaysOnTop = next;
    } catch (error) {
      this.error = reason(error);
    }
  }

  /**
   * Fills the screen, or goes back to the floating size.
   *
   * Rust owns the truth here: it reads the real window state before flipping it,
   * so this stays correct even if the window was snapped full-screen by Windows
   * behind the UI's back.
   */
  async toggleMaximize(): Promise<void> {
    try {
      this.maximized = await bridge.toggleMaximize();
    } catch (error) {
      this.error = reason(error);
    }
  }

  async toggleClickThrough(): Promise<void> {
    const next = !this.clickThrough;
    try {
      await bridge.setClickThrough(next);
      this.clickThrough = next;
    } catch (error) {
      this.error = reason(error);
    }
  }

  async syncNow(): Promise<void> {
    try {
      this.sync = await bridge.syncNow();
    } catch (error) {
      this.error = reason(error);
    }
  }

  async login(serverUrl: string, password: string): Promise<void> {
    this.error = null;
    try {
      this.sync = await bridge.login(serverUrl, password);
      await this.refresh();
    } catch (error) {
      this.error = reason(error);
      throw error;
    }
  }

  async logout(): Promise<void> {
    try {
      this.#close();
      this.sync = await bridge.logout();
    } catch (error) {
      this.error = reason(error);
    }
  }

  /** Version and paths for the settings panel. Fetched once. */
  async loadInfo(): Promise<void> {
    if (this.info) return;
    try {
      this.info = await bridge.appInfo();
    } catch (error) {
      this.error = reason(error);
    }
  }

  /**
   * Re-reads the autostart switch. Unlike {@link loadInfo} this is not cached:
   * the registry is the source of truth and something outside this program may
   * have changed it since the panel was last opened.
   */
  async loadAutostart(): Promise<void> {
    try {
      this.autostart = await bridge.autostart();
    } catch (error) {
      this.error = reason(error);
    }
  }

  async toggleAutostart(): Promise<void> {
    try {
      await bridge.setAutostart(!this.autostart);
    } catch (error) {
      this.error = reason(error);
    }
    // Read back rather than assume: this writes to the registry, and a switch that
    // flipped in the UI while the write failed would be a lie about what happens
    // at the next sign-in.
    await this.loadAutostart();
  }

  #close(): void {
    this.session?.destroy();
    this.session = null;
    this.selectedId = null;
    this.title = "";
    this.connection = "offline";
  }
}
