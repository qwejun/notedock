<script lang="ts">
  import { onMount } from "svelte";
  import type { Editor } from "@tiptap/core";
  import {
    CommandPalette,
    countWords,
    Icon,
    NoteEditor,
    SyncDot,
    type PaletteItem,
  } from "@notedock/editor";
  import Login from "./components/Login.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import { NotesStore } from "./lib/store.svelte";

  const store = new NotesStore();

  let editor = $state<Editor>();
  let words = $state(0);
  let paletteOpen = $state(false);

  const paletteItems = $derived<PaletteItem[]>(
    store.notes.map((note) => ({
      id: note.id,
      title: note.title || "未命名",
      preview: note.preview,
    })),
  );

  onMount(() => {
    if (store.authed) {
      void store.refresh();
      store.start();
    }
    return () => store.stop();
  });

  /*
   * Recount when a different note is opened. Typing is handled in `onChange`, so
   * `session` is the only tracked dependency — the editor instance itself changes
   * with it.
   */
  $effect(() => {
    store.session;
    words = countWords(editor?.getText() ?? "");
  });

  /** Opening a note puts the caret in it, rather than requiring a click. */
  $effect(() => {
    if (store.session && editor) editor.commands.focus("end");
  });

  function onKeydown(event: KeyboardEvent): void {
    if (!(event.ctrlKey || event.metaKey)) return;
    const key = event.key.toLowerCase();

    if (key === "p") {
      event.preventDefault();
      paletteOpen = true;
    }
    // No Ctrl+S: every keystroke is already on its way to the server. Leaving the
    // browser's own handler alone is more honest than swallowing it to do nothing.
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if !store.authed}
  <Login busy={store.busy} message={store.message} onSubmit={(pw) => void store.login(pw)} />
{:else}
  <div class="shell">
    <Sidebar
      notes={store.notes}
      selectedId={store.selectedId}
      onOpen={(id) => store.open(id)}
      onCreate={() => void store.create()}
      onDelete={(id) => void store.remove(id)}
    />

    <main>
      <header>
        <span class="title">{store.title || (store.hasSelection ? "未命名" : "")}</span>
        <span class="meta">
          {#if store.hasSelection}<span class="count">{words} 字</span>{/if}
          <SyncDot status={store.status} withLabel />
          <button class="nd-btn" aria-label="退出登录" onclick={() => store.logout()}>
            <Icon name="close" size={14} />
          </button>
        </span>
      </header>

      {#if store.message}
        <p class="banner" role="status">{store.message}</p>
      {/if}

      {#if store.session}
        <input
          class="note-title"
          type="text"
          aria-label="笔记标题"
          placeholder="标题"
          maxlength="200"
          value={store.title}
          oninput={(event) => store.renameTitle(event.currentTarget.value)}
        />
        <NoteEditor
          session={store.session}
          placeholder="输入笔记内容…"
          onReady={(instance) => (editor = instance)}
          onChange={(text) => (words = countWords(text))}
        />
      {:else}
        <div class="blank">
          <p>还没有打开任何笔记</p>
          <button class="nd-btn nd-btn--primary" onclick={() => void store.create()}>
            新建一篇
          </button>
        </div>
      {/if}
    </main>
  </div>

  <CommandPalette
    bind:open={paletteOpen}
    items={paletteItems}
    onSelect={(id) => store.open(id)}
    onCreate={(title) => void store.create(title)}
    onClose={() => (paletteOpen = false)}
  />
{/if}

<style>
  .shell {
    display: flex;
    height: 100%;
  }

  main {
    display: flex;
    flex-direction: column;
    flex: 1;
    /* Without this the editor's content can push the flex item wider than the
     * viewport and produce a horizontal scrollbar. */
    min-width: 0;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: calc(var(--nd-space) * 3);
    flex: none;
    height: 44px;
    padding: 0 calc(var(--nd-space) * 3) 0 calc(var(--nd-space) * 5);
    border-bottom: 1px solid var(--nd-border);
  }

  .title {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: var(--nd-text-md);
    font-weight: 600;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: calc(var(--nd-space) * 3);
    flex: none;
  }

  .count {
    color: var(--nd-fg-faint);
    font-size: var(--nd-text-xs);
    font-variant-numeric: tabular-nums;
  }

  .banner {
    flex: none;
    margin: 0;
    padding: calc(var(--nd-space) * 2) calc(var(--nd-space) * 5);
    border-bottom: 1px solid var(--nd-border);
    color: var(--nd-fg-dim);
    font-size: var(--nd-text-sm);
  }

  /* The editor is a flex child that must be allowed to shrink, or its internal
   * scroller never engages and the page scrolls instead. */
  .shell :global(.frame) {
    flex: 1;
    min-height: 0;
  }

  .note-title {
    flex: none;
    width: 100%;
    height: 52px;
    padding: 0 calc(var(--nd-space) * 5);
    border: 0;
    border-bottom: 1px solid var(--nd-border);
    background: transparent;
    color: var(--nd-fg);
    font: inherit;
    font-size: var(--nd-text-lg);
    font-weight: 600;
    outline: none;
  }

  .note-title::placeholder { color: var(--nd-fg-faint); }

  .blank {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: calc(var(--nd-space) * 3);
    flex: 1;
    color: var(--nd-fg-faint);
  }

  .blank p {
    margin: 0;
    font-size: var(--nd-text-sm);
  }

  /* The sidebar is a luxury below this width; the palette (Ctrl+P) replaces it. */
  @media (max-width: 640px) {
    .shell :global(.sidebar) {
      display: none;
    }
  }
</style>
