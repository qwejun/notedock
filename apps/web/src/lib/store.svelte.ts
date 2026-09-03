import {
  ApiError,
  NetworkError,
  NoteDockClient,
  NoteSession,
  type ConnectionState,
  type NoteSummary,
  type SyncStatus,
} from "@notedock/editor";

/**
 * Where the bearer token lives. Over the first-stage plain-HTTP deployment the
 * token is already visible to anyone on the path, so `localStorage` is not the
 * weakest link here — but it is reachable by injected script, which is why the
 * app ships no user-supplied HTML and the editor sanitises its own content.
 */
const TOKEN_KEY = "notedock.token";

/** How often the note list is refreshed while a tab is open and focused. */
const LIST_POLL_MS = 5_000;

export class NotesStore {
  #client = new NoteDockClient("", localStorage.getItem(TOKEN_KEY));
  #poll: ReturnType<typeof setInterval> | null = null;
  /** Highest change-log position already folded into {@link notes}. */
  #cursor = 0;

  authed = $state(Boolean(localStorage.getItem(TOKEN_KEY)));
  notes = $state<NoteSummary[]>([]);
  selectedId = $state<string | null>(null);

  /**
   * The open note's collaborative document. Replacing this is what makes the
   * editor rebind; there is no document JSON anywhere in this store.
   */
  session = $state<NoteSession | null>(null);
  title = $state("");

  status = $state<SyncStatus>("synced");
  message = $state<string | null>(null);
  busy = $state(false);

  get hasSelection(): boolean {
    return this.session !== null;
  }

  async login(password: string): Promise<void> {
    this.busy = true;
    this.message = null;
    try {
      const { token } = await this.#client.login(password, "browser");
      localStorage.setItem(TOKEN_KEY, token);
      this.authed = true;
      await this.refresh();
      this.start();
    } catch (error) {
      this.message = describe(error, "登录失败");
      throw error;
    } finally {
      this.busy = false;
    }
  }

  logout(): void {
    this.stop();
    localStorage.removeItem(TOKEN_KEY);
    this.#client.setToken(null);
    this.authed = false;
    this.notes = [];
    this.#cursor = 0;
    this.#close();
  }

  /**
   * Starts polling the metadata feed.
   *
   * Only titles and tombstones need this — note *bodies* arrive over their own
   * WebSocket the instant they change. Which is why a five-second list refresh is
   * not the sync latency anyone experiences.
   */
  start(): void {
    if (this.#poll || !this.authed) return;
    this.#poll = setInterval(() => {
      if (!document.hidden) void this.#pull();
    }, LIST_POLL_MS);
  }

  stop(): void {
    if (!this.#poll) return;
    clearInterval(this.#poll);
    this.#poll = null;
  }

  async refresh(): Promise<void> {
    try {
      this.notes = await this.#client.listNotes();
      this.status = "synced";
      const first = this.notes[0];
      if (!this.selectedId && first) this.open(first.id);
    } catch (error) {
      this.#report(error, "无法加载笔记列表");
    }
  }

  /** Folds in metadata changed since the last cursor. */
  async #pull(): Promise<void> {
    try {
      const { changes, cursor } = await this.#client.sync(this.#cursor);
      this.#cursor = cursor;
      if (changes.length === 0) {
        this.status = "synced";
        return;
      }

      const byId = new Map(this.notes.map((note) => [note.id, note]));
      for (const change of changes) {
        if (change.deleted) byId.delete(change.id);
        else byId.set(change.id, change);
      }
      this.notes = [...byId.values()].sort((a, b) =>
        b.updated_at.localeCompare(a.updated_at),
      );

      // The open note's title is derived server-side, so it arrives here too.
      const open = this.selectedId ? byId.get(this.selectedId) : undefined;
      if (open) this.title = open.title;
      else if (this.selectedId) this.#close(); // deleted elsewhere

      this.status = "synced";
    } catch (error) {
      this.#report(error, "同步失败");
    }
  }

  open(id: string, seedTitle = ""): void {
    if (id === this.selectedId) return;
    this.#close();

    const session = new NoteSession({
      noteId: id,
      cacheKey: `notedock:${id}`,
      ticket: () => this.#client.socketUrl(),
      onState: (state) => this.#onConnection(state),
      initialTitle: this.notes.find((note) => note.id === id)?.title ?? seedTitle,
      onTitle: (title) => (this.title = title),
    });

    // A note created from a name starts with that name as its first line, which
    // is also what the server derives the title from.
    if (seedTitle) {
      void session.whenReady().then(() => session.seedTitle(seedTitle));
    }

    this.selectedId = id;
    this.session = session;
    this.title = this.notes.find((note) => note.id === id)?.title ?? seedTitle;
  }

  renameTitle(title: string): void {
    this.title = title;
    this.session?.setTitle(title);
  }

  async create(title = ""): Promise<void> {
    try {
      // Only metadata: the body the user types goes into the note's Yjs
      // document, which is where every subsequent edit lives too.
      const note = await this.#client.createNote({ id: null, title });
      await this.refresh();
      this.open(note.id, title);
    } catch (error) {
      this.#report(error, "无法新建笔记");
    }
  }

  async remove(id: string): Promise<void> {
    try {
      await this.#client.deleteNote(id);
      if (id === this.selectedId) this.#close();
      await this.refresh();
    } catch (error) {
      this.#report(error, "无法删除这篇笔记");
    }
  }

  /**
   * The sync dot follows the document socket, not the list poller: that is the
   * connection the user's keystrokes actually travel over.
   */
  #onConnection(state: ConnectionState): void {
    this.status = state === "live" ? "synced" : state === "connecting" ? "syncing" : "offline";
  }

  #close(): void {
    this.session?.destroy();
    this.session = null;
    this.selectedId = null;
    this.title = "";
  }

  #report(error: unknown, fallback: string): void {
    if (error instanceof ApiError && error.code === "unauthorized") {
      this.logout();
      this.message = "登录已过期，请重新登录";
      return;
    }
    if (error instanceof NetworkError) this.status = "offline";
    this.message = describe(error, fallback);
  }
}

function describe(error: unknown, fallback: string): string {
  if (error instanceof NetworkError) return error.message;
  if (error instanceof ApiError) return error.message;
  return fallback;
}
