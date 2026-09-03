/** Types shared between components. Declared here rather than inside a
 * component's instance script, which Svelte does not re-export. */

/** A row in the command palette. */
export interface PaletteItem {
  id: string;
  title: string;
  preview: string;
}

/**
 * What the sync dot shows.
 *
 * There is no `conflict` state: concurrent edits to one note merge in the CRDT,
 * so there is never a version for the user to reconcile. Mirrors
 * `notedock_desktop_lib::sync::SyncStatus`.
 */
export type SyncStatus = "synced" | "syncing" | "offline";
