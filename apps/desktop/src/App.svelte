<script lang="ts">
  import { onMount } from "svelte";
  import type { Editor } from "@tiptap/core";
  import {
    CommandPalette,
    NoteEditor,
    NoteTitle,
    type PaletteItem,
  } from "@notedock/editor";
  import TitleBar from "./components/TitleBar.svelte";
  import SettingsPanel from "./components/SettingsPanel.svelte";
  import Login from "./components/Login.svelte";
  import { DesktopStore } from "./lib/store.svelte";
  import { bridge } from "./lib/bridge";

  const store = new DesktopStore();

  let editor = $state<Editor>();
  let paletteOpen = $state(false);
  let settingsOpen = $state(false);
  let busy = $state(false);

  const paletteItems = $derived<PaletteItem[]>(
    store.notes.map((note) => ({
      id: note.id,
      title: note.title || "未命名",
      preview: note.preview,
    })),
  );

  onMount(() => {
    let unlisten: (() => void) | undefined;
    void store.init().then((off) => (unlisten = off));
    return () => unlisten?.();
  });

  $effect(() => {
    document.documentElement.style.setProperty(
      "--nd-window-opacity",
      String(store.opacity),
    );
    document.documentElement.dataset.ndOpaque = store.opacity >= 0.999 ? "true" : "false";
  });

  /* Read by the `.backdrop` rule in app.css, which rounds the full-viewport
     overlays to match the window and must stop doing so when it is square. */
  $effect(() => {
    document.documentElement.dataset.ndMax = store.maximized ? "true" : "false";
  });

  /*
   * Opening or creating a note puts the caret in it. This is a notepad you summon
   * to write in — arriving at a note you then have to click is a wasted step.
   *
   * `session` is the only tracked dependency: the editor is rebuilt when the note
   * changes and not otherwise, so a remote edit arriving mid-sentence cannot move
   * the caret.
   */
  $effect(() => {
    store.session;
    if (store.session && editor) editor.commands.focus("end");
  });

  async function signIn(url: string, password: string): Promise<void> {
    busy = true;
    try {
      await store.login(url, password);
    } catch {
      // The store already holds the message.
    } finally {
      busy = false;
    }
  }

  function onKeydown(event: KeyboardEvent): void {
    // F11 before the modifier check: it is the one window shortcut every other
    // Windows app answers to, and it carries no modifier.
    if (event.key === "F11") {
      event.preventDefault();
      void store.toggleMaximize();
      return;
    }
    if (!(event.ctrlKey || event.metaKey)) return;
    const key = event.key.toLowerCase();

    if (event.shiftKey && key === "k") {
      event.preventDefault();
      void store.toggleClickThrough();
      return;
    }
    if (event.shiftKey) return;

    switch (key) {
      case ",":
        event.preventDefault();
        settingsOpen = true;
        break;
      case "p":
        event.preventDefault();
        paletteOpen = true;
        break;
      case "n":
        event.preventDefault();
        void store.create();
        break;
      // Ctrl+S is muscle memory. There is nothing to save — every keystroke is
      // already on the wire — so it asks the metadata loop to catch up, which is
      // the only thing here that is ever behind.
      case "s":
        event.preventDefault();
        void store.syncNow();
        break;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="window nd-glass" class:full={store.maximized}>
  <TitleBar
    status={store.sync.logged_in ? store.status : "offline"}
    pending={store.sync.logged_in ? store.sync.pending : 0}
    loggedIn={store.sync.logged_in}
    maximized={store.maximized}
    onCreate={() => void store.create()}
    onSwitch={() => (paletteOpen = true)}
    canOpenWeb={store.sync.logged_in && Boolean(store.sync.server_url)}
    onOpenWeb={() => void store.openWeb()}
    onOpenSettings={() => (settingsOpen = true)}
    onToggleMaximize={() => void store.toggleMaximize()}
    onMinimize={() => void bridge.minimizeWindow()}
    onHide={() => void bridge.hideWindow()}
    onQuit={() => void bridge.quit()}
  />

  {#if store.clickThrough}
    <p class="hint" role="status">
      点击穿透已开启。再按 Ctrl+Shift+K 关闭；若窗口已失去焦点，用托盘图标恢复。
    </p>
  {/if}

  {#if store.sync.message}
    <p class="banner" role="status">{store.sync.message}</p>
  {/if}

  {#if store.notice}
    <p class="banner" role="status">{store.notice}</p>
  {/if}

  {#if store.error}
    <p class="banner error" role="alert">{store.error}</p>
  {/if}

  {#if !store.sync.logged_in}
    <Login
      {busy}
      message={store.error}
      initialUrl={store.sync.server_url}
      onSubmit={signIn}
    />
  {:else if store.session}
    <NoteTitle value={store.title} onInput={(title) => store.renameTitle(title)} />
    <NoteEditor
      session={store.session}
      placeholder="输入笔记内容…"
      onReady={(instance) => (editor = instance)}
    />
  {:else}
    <div class="blank">
      <button class="nd-btn nd-btn--primary" onclick={() => void store.create()}>
        新建一篇笔记
      </button>
    </div>
  {/if}
</div>

<CommandPalette
  bind:open={paletteOpen}
  items={paletteItems}
  onSelect={(id) => store.open(id)}
  onCreate={(title) => void store.create(title)}
  onClose={() => (paletteOpen = false)}
/>

<SettingsPanel
  open={settingsOpen}
  {store}
  onClose={() => (settingsOpen = false)}
/>

<style>
  /*
   * The window has no OS chrome, so this element *is* the window: rounded
   * corners, a hairline border and the frosted fill that makes text readable
   * over whatever video is behind it.
   */
  .window {
    display: flex;
    flex-direction: column;
    height: 100%;
    border: 1px solid var(--nd-border-strong);
    border-radius: var(--nd-radius-md);
    box-shadow: var(--nd-shadow-lg);
    overflow: hidden;
  }

  /*
   * Full screen, the window is flush with the work area, so the rounded corners
   * would show the desktop through them and the border would draw a hairline
   * along the screen edge.
   */
  .window.full {
    border: 0;
    border-radius: 0;
    box-shadow: none;
  }

  /* The editor takes everything the header and banners leave. */
  .window :global(.frame) {
    flex: 1;
    min-height: 0;
    background: var(--nd-bg-solid);
  }

  .window :global(.nd-note-title) {
    height: 72px;
    padding: 19px calc(var(--nd-space) * 5) 11px;
    border-bottom: 1px solid var(--nd-border);
    background: var(--nd-bg-solid);
    font-size: 21px;
    font-weight: 700;
    letter-spacing: 0;
  }

  /*
   * The reading column, wide windows only. Padding rather than `max-width`, so the
   * title's rule and the editor's background still run the full width of the window
   * and only the text is capped — a centred 720px card floating on a 1920px screen
   * would read as a dialog, not as the note.
   *
   * `both-edges` is what keeps the two aligned: the scroller then reserves its 8px
   * gutter on each side whether or not the note is long enough to scroll, so
   * `.host`'s box stays centred in the window and its own centred column lands on
   * the same axis as the title's. Without it the body would shift left by 4px the
   * moment a scrollbar appeared.
   */
  .window :global(.scroller) {
    scrollbar-gutter: stable both-edges;
  }

  .window :global(.nd-note-title),
  .window :global(.scroller > .host) {
    padding-inline: max(
      var(--nd-gutter),
      calc((100% - var(--nd-measure)) / 2)
    );
  }

  .window :global(.nd-note-title) {
    --nd-gutter: calc(var(--nd-space) * 5);
  }

  .window :global(.scroller > .host) {
    --nd-gutter: calc(var(--nd-space) * 4);
  }

  .hint,
  .banner {
    flex: none;
    margin: 0;
    padding: calc(var(--nd-space) * 1.5) calc(var(--nd-space) * 3);
    border-bottom: 1px solid var(--nd-border);
    color: var(--nd-fg-dim);
    font-size: var(--nd-text-xs);
    line-height: 1.5;
  }

  .hint {
    color: var(--nd-accent);
    background: var(--nd-accent-soft);
  }

  .banner.error {
    color: var(--nd-danger);
  }

  .blank {
    display: grid;
    place-items: center;
    flex: 1;
    padding: calc(var(--nd-space) * 4);
  }
</style>
