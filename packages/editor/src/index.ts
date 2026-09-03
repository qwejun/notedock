/**
 * Public surface of the shared editor package.
 *
 * Consumed as TypeScript and Svelte source, not as a built bundle: both apps run
 * it through their own Vite pipeline via an alias, which keeps a build step and
 * a dual-package hazard out of the loop.
 */

export { default as NoteEditor } from "./components/NoteEditor.svelte";
export { default as NoteTitle } from "./components/NoteTitle.svelte";
export { default as BubbleToolbar } from "./components/BubbleToolbar.svelte";
export { default as CommandPalette } from "./components/CommandPalette.svelte";
export { default as SyncDot } from "./components/SyncDot.svelte";
export { default as Icon } from "./components/Icon.svelte";

export type { PaletteItem, SyncStatus } from "./lib/types";

export { ApiError, NetworkError, NoteDockClient } from "./lib/api";
export { NoteProvider, type ConnectionState } from "./lib/provider";
export { FRAGMENT, META, NoteSession, TITLE_KEY, type SessionOptions } from "./lib/session";
export {
  countWords,
  createNoteEditor,
  emptyDoc,
  HEADING_LEVELS,
  HIGHLIGHT_COLORS,
  TEXT_COLORS,
  type JSONContent,
  type NoteEditorOptions,
} from "./lib/tiptap";
export { ICON_CIRCLES, ICON_PATHS, type IconCircle, type IconName } from "./lib/icons";

export type * from "./generated/api";
