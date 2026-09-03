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
  /** Initial metadata title and live title updates from this Y.Doc. */
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
  #title: Y.Text;
  #onTitle?: (title: string) => void;
  #titleObserver: () => void;

  constructor(options: SessionOptions) {
    this.noteId = options.noteId;
    this.#title = this.doc.getText("title");
    this.#onTitle = options.onTitle;
    this.#titleObserver = () => this.#onTitle?.(this.title);
    this.#title.observe(this.#titleObserver);

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

    void this.#ready.then(() => {
      if (!this.title && options.initialTitle?.trim()) {
        this.setTitle(options.initialTitle);
      } else {
        this.#onTitle?.(this.title || options.initialTitle?.trim() || "");
      }
    });
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
    return this.#title.toString();
  }

  setTitle(value: string): void {
    const next = value.replace(/[\r\n]+/g, " ").slice(0, 200);
    if (next === this.title) return;
    this.doc.transact(() => {
      this.#title.delete(0, this.#title.length);
      if (next) this.#title.insert(0, next);
    });
  }

  /** True when the document has no content at all, cache included. */
  isEmpty(): boolean {
    return this.fragment.length === 0;
  }

  /**
   * Seeds a brand new note with a heading.
   *
   * Guarded by {@link isEmpty} because seeding twice would duplicate the line:
   * two devices creating the same note offline both think they are first, and
   * Yjs would faithfully keep both copies.
   */
  seedTitle(text: string): void {
    if (!this.title) this.setTitle(text.trim());
  }

  destroy(): void {
    this.#title.unobserve(this.#titleObserver);
    this.#provider.destroy();
    void this.#cache?.destroy();
    this.doc.destroy();
  }
}
