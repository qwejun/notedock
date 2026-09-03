import * as Y from "yjs";
import { IndexeddbPersistence } from "y-indexeddb";
import { NoteProvider, type ConnectionState } from "./provider";

/**
 * Fragment name inside the Y.Doc.
 *
 * Must match both `@tiptap/extension-collaboration`'s default `field` and
 * `notedock_server::ydoc::ROOT`. All three have to agree or the server would
 * materialize an empty document while the editor happily edited another one.
 */
export const FRAGMENT = "default";

/**
 * Map of the note's own metadata, and the key inside it holding the title.
 *
 * Must match `notedock_server::ydoc::{META, TITLE}`.
 */
export const META = "meta";
export const TITLE_KEY = "title";

export interface SessionOptions {
  noteId: string;
  /** Fetches a fresh single-use WebSocket URL. See {@link NoteProvider}. */
  ticket: () => Promise<string>;
  /**
   * Where to cache the document locally. Unset disables caching, which is what
   * the desktop app wants — its Rust side owns the on-disk copy.
   */
  cacheKey?: string;
  onState?: (state: ConnectionState) => void;
  /**
   * Title from the metadata list. Shown immediately, and adopted into the
   * document once the server has confirmed the document has no title of its own.
   */
  initialTitle?: string;
  onTitle?: (title: string) => void;
}

/**
 * One note's collaborative document.
 *
 * Two independent things keep it alive: IndexedDB, so the note opens instantly
 * and survives a reload with no network; and a {@link NoteProvider}, so it
 * converges with everyone else. Neither knows about the other — both just apply
 * updates to the same Y.Doc, and Yjs makes the order irrelevant.
 */
export class NoteSession {
  readonly doc = new Y.Doc();
  readonly noteId: string;

  #provider: NoteProvider;
  #cache: IndexeddbPersistence | null = null;
  /** Resolves once the local cache has been read, if there is one. */
  #ready: Promise<void>;
  #meta: Y.Map<unknown>;
  #onTitle?: (title: string) => void;
  #metaObserver: (event: Y.YMapEvent<unknown>) => void;
  #destroyed = false;

  constructor(options: SessionOptions) {
    this.noteId = options.noteId;
    this.#meta = this.doc.getMap(META);
    this.#onTitle = options.onTitle;
    this.#metaObserver = (event) => {
      if (event.keysChanged.has(TITLE_KEY)) this.#onTitle?.(this.title);
    };
    this.#meta.observe(this.#metaObserver);

    if (options.cacheKey) {
      const cache = new IndexeddbPersistence(options.cacheKey, this.doc);
      this.#cache = cache;
      this.#ready = cache.whenSynced.then(() => undefined);
    } else {
      this.#ready = Promise.resolve();
    }

    this.#provider = new NoteProvider({
      doc: this.doc,
      noteId: options.noteId,
      ticket: options.ticket,
      onState: options.onState,
    });

    const initial = options.initialTitle?.trim() ?? "";

    // Showing the metadata title costs nothing and writes nothing. Worth doing
    // separately from adopting it, because a note that has a name should not sit
    // under an empty header while the socket comes up.
    void this.#ready.then(() => {
      if (!this.#destroyed) this.#onTitle?.(this.title || initial);
    });

    // Adopting it into the document has to wait for the *server*, not the cache.
    // `#ready` only covers IndexedDB, so a client opening a note it had never
    // cached found no title, wrote the one from the list, and then received the
    // server's own copy — two independent values for one field, which is how
    // "blender学习" came back as "blender学习blender学习".
    if (initial) {
      void this.#provider.whenSynced().then(() => {
        if (!this.#destroyed && !this.title) this.setTitle(initial);
      });
    }
  }

  /** The XML fragment TipTap's Collaboration extension binds to. */
  get fragment(): Y.XmlFragment {
    return this.doc.getXmlFragment(FRAGMENT);
  }

  /**
   * Awaits the local cache. The editor can mount before this resolves — Yjs
   * merges whatever arrives later — but waiting avoids a visible flash of empty
   * document on a note that is already cached.
   */
  whenReady(): Promise<void> {
    return this.#ready;
  }

  get state(): ConnectionState {
    return this.#provider.state;
  }

  get title(): string {
    const value = this.#meta.get(TITLE_KEY);
    return typeof value === "string" ? value : "";
  }

  /**
   * Renames the note.
   *
   * One map entry, replaced whole, rather than a collaborative string. Two
   * devices renaming at once therefore settle on one of the two names instead of
   * interleaving them, and — the reason this changed — a title can no longer end
   * up written twice: concurrent inserts into a shared `Y.Text` both survive by
   * design, which for a 200-character field is a bug rather than a feature.
   */
  setTitle(value: string): void {
    const next = value.replace(/[\r\n]+/g, " ").slice(0, 200);
    if (next === this.title) return;
    this.#meta.set(TITLE_KEY, next);
  }

  destroy(): void {
    this.#destroyed = true;
    this.#meta.unobserve(this.#metaObserver);
    this.#provider.destroy();
    void this.#cache?.destroy();
    this.doc.destroy();
  }
}
