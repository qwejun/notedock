//! Reading every note out of the system as Markdown.
//!
//! A note's body is a Yjs document, not a row in a table, so "export all notes"
//! is not a query — each document has to be brought up to date before it can be
//! read. That is what this file does: one note at a time, reusing the one the
//! user has open and briefly opening a session for each of the rest.

import { yXmlFragmentToProseMirrorRootNode } from "y-prosemirror";
import type * as Y from "yjs";
import type { JSONContent } from "@tiptap/core";
import { NoteSession } from "./session";
import { noteSchema } from "./tiptap";
import { toMarkdown } from "./markdown";

/** How long one note may take before it is exported from cache alone. */
const SYNC_TIMEOUT_MS = 6000;

/** Consecutive unreachable notes after which the wait is abandoned. */
const GIVE_UP_AFTER = 2;

export interface ExportedNote {
  title: string;
  markdown: string;
}

export interface ExportAllOptions {
  /** The note list, in the order the files should be written. */
  notes: readonly { id: string; title: string }[];
  /** Fetches a fresh single-use WebSocket URL. See {@link NoteSession}. */
  ticket: () => Promise<string>;
  /**
   * The note the user has open, if any. Reused rather than reopened: it is
   * already live, and a second socket for the same note would sync a document
   * this one already has.
   */
  live?: NoteSession | null;
  /** Where each note caches locally, so a cached note exports while offline. */
  cacheKey?: (id: string) => string;
  /** Reports progress, because this waits on the network once per note. */
  onProgress?: (done: number, total: number) => void;
}

/** A Y.Doc body as TipTap JSON, without mounting an editor to read it. */
export function fragmentToJSON(fragment: Y.XmlFragment): JSONContent {
  return yXmlFragmentToProseMirrorRootNode(
    fragment,
    noteSchema(),
  ).toJSON() as JSONContent;
}

function markdownOf(session: NoteSession, fallbackTitle: string): ExportedNote {
  // The document's own title wins: the metadata list is materialized on a timer
  // server-side, so a note renamed a moment ago is still listed under its old
  // name.
  const title = session.title || fallbackTitle;
  return {
    title,
    markdown: toMarkdown(fragmentToJSON(session.fragment), title),
  };
}

/**
 * Waits for the server, but not forever.
 *
 * An export that hangs because one note's socket never came up is worse than an
 * export that writes what the local cache had — so this resolves either way, and
 * reports which happened.
 */
async function settle(session: NoteSession, timeoutMs: number): Promise<boolean> {
  if (timeoutMs <= 0) {
    await session.whenReady();
    return false;
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      Promise.all([session.whenReady(), session.whenSynced()]).then(() => true),
      new Promise<boolean>((resolve) => {
        timer = setTimeout(() => resolve(false), timeoutMs);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

/**
 * Every note as Markdown, in list order.
 *
 * Sequential on purpose. Each note costs a ticket and a socket, and doing twenty
 * at once would open twenty rooms on the server to save a second or two on an
 * action the user takes rarely.
 */
export async function exportAll(
  options: ExportAllOptions,
): Promise<ExportedNote[]> {
  const out: ExportedNote[] = [];
  const total = options.notes.length;
  let misses = 0;

  for (const [index, note] of options.notes.entries()) {
    options.onProgress?.(index, total);

    if (options.live?.noteId === note.id) {
      out.push(markdownOf(options.live, note.title));
      continue;
    }

    // No `initialTitle`: adopting one *writes* to the document, and an export
    // must not edit what it is reading.
    const session = new NoteSession({
      noteId: note.id,
      cacheKey: options.cacheKey?.(note.id),
      ticket: options.ticket,
    });
    try {
      // Two notes in a row that never heard from the server means the server is
      // not there, so stop paying the timeout for every remaining note — with the
      // network down that is the difference between twelve seconds and six per
      // note. What is cached still exports.
      const synced = await settle(
        session,
        misses >= GIVE_UP_AFTER ? 0 : SYNC_TIMEOUT_MS,
      );
      misses = synced ? 0 : misses + 1;
      out.push(markdownOf(session, note.title));
    } finally {
      session.destroy();
    }
  }

  options.onProgress?.(total, total);
  return out;
}
