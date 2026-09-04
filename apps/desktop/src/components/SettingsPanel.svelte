<script lang="ts">
  import { Icon, SyncDot } from "@notedock/editor";
  import { bridge } from "../lib/bridge";
  import type { DesktopStore } from "../lib/store.svelte";

  interface Props {
    open: boolean;
    store: DesktopStore;
    onClose: () => void;
  }

  let { open, store, onClose }: Props = $props();

  let panel = $state<HTMLDivElement>();

  const SYNC_LABELS = {
    synced: "已同步",
    syncing: "同步中",
    offline: "离线",
  } as const;

  const percent = $derived(Math.round(store.opacity * 100));
  const canExport = $derived(store.notes.length > 0 && !store.exporting);

  $effect(() => {
    if (!open) return;
    void store.loadInfo();
    // Not cached like the app info: the registry is what decides this, and 任务
    // 管理器 can have turned it off since the last time the panel was open.
    void store.loadAutostart();
    // Focus the sheet so Escape and Tab land inside it rather than in the editor
    // underneath.
    requestAnimationFrame(() => panel?.focus());
  });

  /** Runs an action and closes, for the ones that leave nothing to look at. */
  function act(run: () => void): void {
    run();
    onClose();
  }

  /*
   * Closes first, deliberately. The export walks every note over its own socket,
   * so it reports progress in the window's own banner — which is behind this
   * sheet.
   */
  function startExport(): void {
    act(() => void store.exportNotes());
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (open && event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }}
/>

{#if open}
  <div
    class="backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <div
      bind:this={panel}
      class="sheet nd-glass"
      role="dialog"
      aria-modal="true"
      aria-label="设置"
      tabindex="-1"
    >
      <header>
        <span class="heading">设置</span>
        <button class="nd-btn" aria-label="关闭设置" onclick={onClose}>
          <Icon name="close" size={14} />
        </button>
      </header>

      <div class="body">
        <!-- First, because when the program runs comes before how its window
             behaves. One row today; anything else about launching belongs here. -->
        <section aria-labelledby="nd-set-launch">
          <h2 id="nd-set-launch">启动</h2>

          <div class="field">
            <span class="text">
              <span class="name" id="nd-autostart">开机自启动</span>
              <span class="note">登录 Windows 后自动打开，回到上次那篇笔记</span>
            </span>
            <button
              class="switch"
              role="switch"
              aria-checked={store.autostart}
              aria-labelledby="nd-autostart"
              onclick={() => void store.toggleAutostart()}
            ><span class="knob"></span></button>
          </div>
        </section>

        <section aria-labelledby="nd-set-window">
          <h2 id="nd-set-window">窗口</h2>

          <div class="field field--stack">
            <label class="text" for="nd-opacity">
              <span class="name">不透明度</span>
            </label>
            <span class="value">{percent}%</span>
            <input
              id="nd-opacity"
              class="slider"
              type="range"
              min="0.3"
              max="1"
              step="0.05"
              value={store.opacity}
              oninput={(event) =>
                store.previewOpacity(Number(event.currentTarget.value))}
              onchange={(event) =>
                void store.commitOpacity(Number(event.currentTarget.value))}
            />
          </div>

          <div class="field">
            <span class="text">
              <span class="name" id="nd-ontop">窗口置顶</span>
              <span class="note">浮在其他窗口之上</span>
            </span>
            <button
              class="switch"
              role="switch"
              aria-checked={store.alwaysOnTop}
              aria-labelledby="nd-ontop"
              onclick={() => void store.toggleAlwaysOnTop()}
            ><span class="knob"></span></button>
          </div>

          <div class="field">
            <span class="text">
              <span class="name" id="nd-through">点击穿透</span>
              <span class="note">鼠标穿过窗口 · Ctrl+Shift+K 切换</span>
            </span>
            <button
              class="switch"
              role="switch"
              aria-checked={store.clickThrough}
              aria-labelledby="nd-through"
              onclick={() => void store.toggleClickThrough()}
            ><span class="knob"></span></button>
          </div>
        </section>

        <section aria-labelledby="nd-set-sync">
          <h2 id="nd-set-sync">同步</h2>

          <div class="field">
            <span class="text">
              <span class="name">状态</span>
              <span class="note mono" title={store.sync.server_url}>
                {store.sync.server_url || "未连接"}
              </span>
            </span>
            <span class="state">
              <SyncDot status={store.status} />
              <span>{SYNC_LABELS[store.status]}</span>
            </span>
          </div>

          {#if store.sync.pending > 0}
            <div class="field">
              <span class="text"><span class="name">待上传</span></span>
              <span class="value">{store.sync.pending} 处改动</span>
            </div>
          {/if}

          <div class="field">
            <span class="text">
              <span class="name">退出登录</span>
              <span class="note">本地笔记保留</span>
            </span>
            <button
              class="btn danger"
              onclick={() => act(() => void store.logout())}>退出</button
            >
          </div>
        </section>

        <section aria-labelledby="nd-set-data">
          <h2 id="nd-set-data">数据</h2>

          <div class="field" class:off={!canExport}>
            <span class="text">
              <span class="name">导出全部笔记</span>
              <span class="note">
                {store.notes.length} 篇 · Markdown，存到 文档\NoteDock\
              </span>
            </span>
            <button class="btn" disabled={!canExport} onclick={startExport}>
              {store.exporting ? "导出中…" : "导出"}
            </button>
          </div>
        </section>

        <!-- Reference, not settings: kept at the bottom and visually quieter so
             it stops competing with the controls above. The version goes last of
             all — it is what you come here to read when something is wrong, and
             the bottom of the sheet is where you look for it. -->
        <footer>
          <button class="btn danger quit" onclick={() => void bridge.quit()}>
            <Icon name="close" size={13} />
            退出 NoteDock
          </button>
          <div class="meta">
            <span class="version">版本 {store.info?.version ?? "…"}</span>
            <span class="mono path" title={store.info?.data_dir ?? ""}>
              {store.info?.data_dir ?? "…"}
            </span>
          </div>
        </footer>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    background: rgb(0 0 0 / 30%);
  }

  /* A full sheet rather than an anchored popover: the window can be 250px wide,
     which leaves no room for a dropdown to hang off anything. */
  .sheet {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    outline: none;
  }

  /* Same height as the title bar it covers, so opening settings does not shift
     the content baseline. */
  header {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    height: 48px;
    padding: 0 calc(var(--nd-space) * 3);
    border-bottom: 1px solid var(--nd-border);
  }

  .heading {
    font-size: var(--nd-text-sm);
    font-weight: 700;
    color: var(--nd-fg);
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: calc(var(--nd-space) * 3);
  }

  /* Full screen the window is 1920px wide, and a row with its label against one
     edge and its switch against the other is unreadable. Cap the column and centre
     it — the scrollbar stays at the window edge, and at 400px nothing moves. */
  section,
  footer {
    max-width: 620px;
    margin-inline: auto;
  }

  section + section {
    margin-top: calc(var(--nd-space) * 4);
  }

  h2 {
    margin: 0 0 var(--nd-space);
    padding: 0 calc(var(--nd-space) * 2);
    color: var(--nd-fg-faint);
    font-size: var(--nd-text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
  }

  /* One anatomy for every row: text on the left, exactly one control on the
     right. A grid, not flex-wrap, so the control keeps its own column and never
     drops underneath the label when a CJK hint runs long. */
  .field {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: calc(var(--nd-space) * 2);
    padding: calc(var(--nd-space) * 2);
    border-radius: var(--nd-radius-sm);
    font-size: var(--nd-text-sm);
  }

  /* The slider needs the full width, so it takes a second row of its own. */
  .field--stack .slider {
    grid-column: 1 / -1;
    margin-top: var(--nd-space);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .name {
    color: var(--nd-fg);
  }

  /* Tighter than --nd-leading: two stacked lines, not prose. */
  .note {
    color: var(--nd-fg-faint);
    font-size: var(--nd-text-xs);
    line-height: 1.45;
  }

  .off .name {
    color: var(--nd-fg-dim);
  }

  /* Read-only values: dimmer and never interactive-looking. */
  .value {
    color: var(--nd-fg-dim);
    font-size: var(--nd-text-xs);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .state {
    display: flex;
    align-items: center;
    gap: calc(var(--nd-space) * 1.5);
    color: var(--nd-fg-dim);
    font-size: var(--nd-text-xs);
    white-space: nowrap;
  }

  .mono {
    font-family: var(--nd-font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .path {
    white-space: normal;
    overflow-wrap: anywhere;
  }

  .slider {
    width: 100%;
    accent-color: var(--nd-accent);
  }

  /* Bordered, so an action is legible as pressable without hovering it first —
     the old rows were indistinguishable from the read-only ones. */
  .btn {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: calc(var(--nd-space) * 1.5);
    height: 26px;
    padding: 0 calc(var(--nd-space) * 2.5);
    border: 1px solid var(--nd-border-strong);
    border-radius: var(--nd-radius-sm);
    background: none;
    color: var(--nd-fg);
    font: inherit;
    font-size: var(--nd-text-xs);
    cursor: pointer;
    transition: background var(--nd-duration) var(--nd-ease);
  }

  .btn:hover:not(:disabled) {
    background: var(--nd-bg-hover);
  }

  .btn:disabled {
    cursor: default;
    opacity: 0.45;
  }

  .btn.danger {
    color: var(--nd-danger);
  }

  footer {
    display: flex;
    flex-direction: column;
    gap: calc(var(--nd-space) * 2);
    margin-top: calc(var(--nd-space) * 4);
    padding: calc(var(--nd-space) * 3) calc(var(--nd-space) * 2) 0;
    border-top: 1px solid var(--nd-border);
  }

  .meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    color: var(--nd-fg-faint);
    font-size: var(--nd-text-xs);
  }

  /* One step brighter than the path beside it: the version is the line people are
     actually sent here to read. */
  .version {
    color: var(--nd-fg-dim);
  }

  .quit {
    width: 100%;
    height: 30px;
  }

  .switch {
    position: relative;
    flex: none;
    width: 30px;
    height: 18px;
    padding: 0;
    border: none;
    border-radius: 999px;
    background: var(--nd-border-strong);
    cursor: pointer;
    transition: background var(--nd-duration) var(--nd-ease);
  }

  .switch[aria-checked="true"] {
    background: var(--nd-accent);
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 999px;
    background: #fff;
    transition: transform var(--nd-duration) var(--nd-ease);
  }

  .switch[aria-checked="true"] .knob {
    transform: translateX(12px);
  }
</style>

