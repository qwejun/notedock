<script lang="ts">
  import { onMount } from "svelte";
  import type { Editor } from "@tiptap/core";
  import { createNoteEditor } from "../lib/tiptap";
  import type { NoteSession } from "../lib/session";
  import BubbleToolbar from "./BubbleToolbar.svelte";

  interface Props {
    /**
     * The note being edited. Changing this tears the editor down and rebuilds it
     * against the new document — there is no way to rebind a live ProseMirror
     * view to a different Y.Doc fragment.
     */
    session: NoteSession;
    editable?: boolean;
    placeholder?: string;
    /** Plain text after every change, local or remote. For the word count. */
    onChange?: (text: string) => void;
    onImageDrop?: (file: File) => Promise<string | null>;
    onReady?: (editor: Editor) => void;
  }

  let {
    session,
    editable = true,
    placeholder,
    onChange,
    onImageDrop,
    onReady,
  }: Props = $props();

  let frame = $state<HTMLDivElement>();
  let host = $state<HTMLDivElement>();
  let editor = $state<Editor>();

  /*
   * Rebuilt whenever the session changes. There is no `setContent` here and no
   * `docKey` to compare: the Y.Doc *is* the state, so a remote edit arrives as a
   * ProseMirror transaction that preserves the caret instead of replacing the
   * document under it. That is the whole reason for the Yjs migration.
   */
  $effect(() => {
    const current = session;
    const element = host;
    if (!element) return;

    const instance = createNoteEditor({
      element,
      session: current,
      editable,
      placeholder,
      onImageDrop,
      onUpdate: (text) => onChange?.(text),
    });

    editor = instance;
    onReady?.(instance);

    return () => {
      instance.destroy();
      if (editor === instance) editor = undefined;
    };
  });

  $effect(() => {
    editor?.setEditable(editable);
  });
</script>

<div class="frame" bind:this={frame}>
  <div class="scroller">
    <div class="host" bind:this={host}></div>
  </div>
  {#if editor}
    <BubbleToolbar {editor} container={frame} />
  {/if}
</div>

<style>
  /*
   * Two nested boxes on purpose: `.frame` is the positioning context for the
   * bubble bar and does not scroll, `.scroller` does. If the bar lived inside
   * the scrolling box its absolute coordinates would drift by scrollTop.
   */
  .frame {
    position: relative;
    min-width: 0;
    min-height: 0;
    height: 100%;
    overflow: hidden;
  }

  .scroller {
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .host {
    min-height: 100%;
    padding: calc(var(--nd-space) * 3) calc(var(--nd-space) * 4);
  }

  /* The ProseMirror element itself is created by TipTap inside `.host`, so it
   * cannot be reached by scoped selectors. */
  :global(.nd-prose) {
    min-height: 100%;
  }
</style>
