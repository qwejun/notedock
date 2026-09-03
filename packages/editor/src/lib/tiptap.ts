import { Editor, type JSONContent } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import Collaboration from "@tiptap/extension-collaboration";
import { Color, TextStyle } from "@tiptap/extension-text-style";
import Highlight from "@tiptap/extension-highlight";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
import type { NoteSession } from "./session";

/**
 * High-contrast text colours, carried over from the V1 toolbar. Values are
 * chosen to stay legible on both the dark and the light surface, since a note
 * written in one theme is often read in the other.
 */
export const TEXT_COLORS = [
  { label: "红", value: "#e2555b" },
  { label: "橙", value: "#d97a2b" },
  { label: "黄", value: "#b8951f" },
  { label: "绿", value: "#2f9e5c" },
  { label: "青", value: "#1c96a6" },
  { label: "蓝", value: "#3b7fe0" },
  { label: "紫", value: "#7d5fd8" },
  { label: "粉", value: "#cf4d90" },
] as const;

/**
 * Pastel highlights. `prose.css` forces dark text inside `<mark>`, so these
 * read the same in either theme.
 */
export const HIGHLIGHT_COLORS = [
  { label: "黄", value: "#fde68a" },
  { label: "薄荷", value: "#a7f3d0" },
  { label: "粉", value: "#fbcfe8" },
  { label: "天蓝", value: "#bfdbfe" },
  { label: "薰衣草", value: "#ddd6fe" },
] as const;

export const HEADING_LEVELS = [1, 2, 3] as const;

/**
 * An empty TipTap document. Mirrors `notedock_api::empty_doc`.
 *
 * Collaborative editing never goes through JSON — the Y.Doc is the body. This is
 * for the REST surface that still carries a snapshot: creating a note, and the
 * offline/bootstrap fallback.
 */
export function emptyDoc(): JSONContent {
  return { type: "doc", content: [] };
}

export interface NoteEditorOptions {
  element: HTMLElement;
  /**
   * The collaborative document this editor edits. Content is never passed in as
   * JSON: the Y.Doc is the single source of truth, and TipTap's Collaboration
   * extension binds directly to it.
   */
  session: NoteSession;
  placeholder?: string;
  editable?: boolean;
  /**
   * Fires after every change, local or remote. Carries plain text only — the
   * document body belongs to Yjs, so there is nothing for a caller to persist.
   * Used for the word count and to know the note has been touched.
   */
  onUpdate?: (text: string) => void;
  /** Fires when the selection or active marks change, to refresh the toolbar. */
  onStateChange?: () => void;
  /**
   * Uploads a pasted or dropped image and resolves to the URL to reference.
   * Left unset until the blob endpoints exist, in which case image paste falls
   * through to ProseMirror's default handling.
   */
  onImageDrop?: (file: File) => Promise<string | null>;
}

export function createNoteEditor(options: NoteEditorOptions): Editor {
  // The annotation is required, not cosmetic: `editorProps` below closes over
  // `editor`, so without it TypeScript infers the type from its own initializer
  // and gives up with an implicit `any`.
  const editor: Editor = new Editor({
    element: options.element,
    editable: options.editable ?? true,
    // No `content`: the Collaboration extension populates the editor from the
    // Y.Doc. Passing content here would inject a second copy of the document.
    extensions: [
      // StarterKit in TipTap 3 already bundles Link, Underline, lists, code
      // blocks and undo history, so those are configured rather than re-added —
      // adding them twice is a duplicate-extension warning.
      StarterKit.configure({
        link: {
          openOnClick: false,
          autolink: true,
          // A note is user-authored content, but it also syncs between devices;
          // refuse anything that is not a plain web or mail link.
          protocols: ["http", "https", "mailto"],
        },
        heading: { levels: [...HEADING_LEVELS] },
        // Collaboration brings its own history, backed by the CRDT, so that undo
        // only ever reverts *your* edits and not a collaborator's.
        undoRedo: false,
      }),
      Collaboration.configure({ fragment: options.session.fragment }),
      TextStyle,
      Color,
      Highlight.configure({ multicolor: true }),
      Image.configure({ inline: false, allowBase64: false }),
      Placeholder.configure({
        placeholder: options.placeholder ?? "写点什么…",
      }),
    ],
    editorProps: {
      attributes: {
        class: "nd-prose",
        // The window is frameless and the editor fills it; spell-check squiggles
        // are noise at this size.
        spellcheck: "false",
      },
      handlePaste: (_view, event) => handleFiles(editor, event.clipboardData, options),
      handleDrop: (_view, event) => {
        const transfer = (event as DragEvent).dataTransfer;
        return handleFiles(editor, transfer, options);
      },
    },
    onUpdate: ({ editor }) => {
      options.onUpdate?.(editor.getText());
      options.onStateChange?.();
    },
    onSelectionUpdate: () => options.onStateChange?.(),
    onTransaction: () => options.onStateChange?.(),
  });

  return editor;
}

/**
 * Pulls image files out of a paste or drop.
 *
 * Deliberately reads `DataTransfer` from the event rather than calling
 * `navigator.clipboard.read()`: the async Clipboard API requires a secure
 * context, and the first-stage deployment is plain HTTP. The paste event has no
 * such restriction, so screenshot pasting keeps working in the browser client.
 */
function handleFiles(
  editor: Editor,
  transfer: DataTransfer | null,
  options: NoteEditorOptions,
): boolean {
  const upload = options.onImageDrop;
  if (!upload) return false;

  const images = Array.from(transfer?.files ?? []).filter((file) =>
    file.type.startsWith("image/"),
  );
  if (images.length === 0) return false;

  void (async () => {
    for (const file of images) {
      const src = await upload(file);
      if (src) editor.chain().focus().setImage({ src }).run();
    }
  })();

  // Returning true tells ProseMirror we took care of it, which also stops the
  // browser from inserting its own base64 copy of the image.
  return true;
}

/**
 * Counts CJK characters individually and everything else by whitespace.
 * Splitting on whitespace alone reports "1" for any all-Chinese note, which is
 * the bug this replaces in V1.
 */
export function countWords(text: string): number {
  const trimmed = text.trim();
  if (!trimmed) return 0;

  const cjk = /[一-鿿㐀-䶿豈-﫿぀-ヿ가-힯]/g;
  const cjkCount = trimmed.match(cjk)?.length ?? 0;
  const rest = trimmed.replace(cjk, " ");
  const words = rest.split(/\s+/).filter(Boolean).length;

  return cjkCount + words;
}

export type { JSONContent };
