<script lang="ts">
  import { Icon, SyncDot } from "@notedock/editor";
  import { bridge } from "../lib/bridge";
  import type { DesktopStore } from "../lib/store.svelte";

  interface Props {
    open: boolean;
    store: DesktopStore;
    /** Opening the command palette belongs to the window, not to this panel. */
    onSearch: () => void;
    onClose: () => void;
  }

  let { open, store, onSearch, onClose }: Props = $props();

  let panel = $state<HTMLDivElement>();

  const SYNC_LABELS = {
    synced: "已同步",
    syncing: "同步中",
    offline: "离线",
  } as const;

  /** Only a note that is actually open can be pinned to the window. */
  const canSpotlight = $derived(store.hasSelection);
  const spotlit = $derived(
    store.selectedId !== null && store.selectedId === store.spotlightId,
  );
  const percent = $derived(Math.round(store.opacity * 100));

  $effect(() => {
    if (!open) return;
    void store.loadInfo();
    // Focus the sheet so Escape and Tab land inside it rather than in the editor
    // underneath.
    requestAnimationFrame(() => panel?.focus());
  });

  /** Runs an action and closes, for the rows that are one-shot commands. */
  function act(run: () => void): void {
    run();
    onClose();
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
        <section aria-labelledby="nd-set-window">
          <h2 id="nd-set-window">窗口</h2>

          <div class="row">
            <label class="label" for="nd-opacity">不透明度</label>
            <span class="value">{percent}%</span>
            <input
              id="nd-opacity"
              class="slider"
              type="range"
              min="0.3"
              max="1"
              step="0.05"
              value={store.opacity}
              oninput={(event) => store.previewOpacity(Number(event.currentTarget.value))}
              onchange={(event) => void store.commitOpacity(Number(event.currentTarget.value))}
            />
          </div>

          <div class="row">
            <span class="label" id="nd-ontop">窗口置顶</span>
            <span class="hint">浮在其他窗口之上</span>
            <button
              class="switch"
              role="switch"
              aria-checked={store.alwaysOnTop}
              aria-labelledby="nd-ontop"
              onclick={() => void store.toggleAlwaysOnTop()}
            ><span class="knob"></span></button>
          </div>

          <div class="row">
            <span class="label" id="nd-through">点击穿透</span>
            <span class="hint">鼠标穿过窗口，Ctrl+Shift+K 也可切换</span>
            <button
              class="switch"
              role="switch"
              aria-checked={store.clickThrough}
              aria-labelledby="nd-through"
              onclick={() => void store.toggleClickThrough()}
            ><span class="knob"></span></button>
          </div>
        </section>

        <section aria-labelledby="nd-set-notes">
          <h2 id="nd-set-notes">笔记</h2>

          {#if canSpotlight}
            <div class="row">
              <span class="label" id="nd-spot">桌面置顶这篇</span>
              <span class="hint">下次启动直接打开它</span>
              <button
                class="switch"
                role="switch"
                aria-checked={spotlit}
                aria-labelledby="nd-spot"
                onclick={() => void store.toggleSpotlight()}
              ><span class="knob"></span></button>
            </div>
          {/if}

          <button class="action" onclick={() => act(() => void store.create())}>
            <Icon name="plus" size={14} />
            <span class="label">新建笔记</span>
          </button>

          <button class="action" onclick={() => act(onSearch)}>
            <Icon name="search" size={14} />
            <span class="label">搜索并切换笔记</span>
          </button>
        </section>

        <section aria-labelledby="nd-set-sync">
          <h2 id="nd-set-sync">同步</h2>

          <div class="row">
            <span class="label">服务器</span>
            <span class="mono" title={store.sync.server_url}>
              {store.sync.server_url || "未连接"}
            </span>
          </div>

          <div class="row">
            <span class="label">状态</span>
            <span class="status">
              <SyncDot status={store.status} />
              <span>{SYNC_LABELS[store.status]}</span>
              {#if store.sync.pending > 0}
                <span class="hint">{store.sync.pending} 处待上传</span>
              {/if}
            </span>
          </div>

          <button class="action" onclick={() => void store.syncNow()}>
            <span class="label">立即同步</span>
          </button>

          <button class="action" onclick={() => act(() => void store.logout())}>
            <span class="label">退出登录</span>
            <span class="hint">本地笔记保留</span>
          </button>
        </section>

        <section aria-labelledby="nd-set-about">
          <h2 id="nd-set-about">关于</h2>

          <div class="row">
            <span class="label">版本</span>
            <span class="mono">{store.info?.version ?? "…"}</span>
          </div>

          <div class="row stacked">
            <span class="label">本地数据</span>
            <!-- Worth surfacing: this is where the offline cache and the bearer
                 token live, and nobody should have to guess. -->
            <span class="mono path" title={store.info?.data_dir ?? ""}>
              {store.info?.data_dir ?? "…"}
            </span>
          </div>

        </section>

        <button class="action danger" onclick={() => void bridge.quit()}>
          <Icon name="close" size={14} />
          <span class="label">退出 NoteDock</span>
        </button>
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

  /*
   * Fills the window rather than floating as a popover: at the 250×200 minimum
   * there is no room for an anchored dropdown, and a full sheet that scrolls
   * behaves the same at every size.
   */
  .sheet {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    outline: none;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex: none;
    height: 28px;
    padding: 0 calc(var(--nd-space) * 2);
    border-bottom: 1px solid var(--nd-border);
  }

  .heading {
    padding-left: calc(var(--nd-space) * 1.5);
    font-size: var(--nd-text-xs);
    font-weight: 600;
    color: var(--nd-fg-dim);
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: calc(var(--nd-space) * 2);
  }

  section {
    margin-bottom: calc(var(--nd-space) * 3);
  }

  h2 {
    margin: 0 0 var(--nd-space);
    padding: 0 calc(var(--nd-space) * 1.5);
    color: var(--nd-fg-faint);
    font-size: var(--nd-text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  /* Rows wrap so a long label plus a control never forces sideways scrolling in
   * a 250px window. */
  .row,
  .action {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: calc(var(--nd-space) * 2);
    width: 100%;
    padding: calc(var(--nd-space) * 1.5);
    border: none;
    border-radius: var(--nd-radius-sm);
    background: none;
    color: var(--nd-fg);
    font: inherit;
    font-size: var(--nd-text-sm);
    text-align: left;
  }

  .action {
    cursor: pointer;
  }

  .action:hover {
    background: var(--nd-bg-hover);
  }

  .action.danger:hover {
    color: var(--nd-danger);
  }

  .row.stacked {
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }

  .label {
    flex: 1;
    min-width: 0;
  }

  .hint {
    flex: none;
    color: var(--nd-fg-faint);
    font-size: var(--nd-text-xs);
  }

  .value {
    flex: none;
    color: var(--nd-fg-dim);
    font-size: var(--nd-text-xs);
    font-variant-numeric: tabular-nums;
  }

  .mono {
    flex: none;
    max-width: 100%;
    overflow: hidden;
    color: var(--nd-fg-dim);
    font-family: var(--nd-font-mono);
    font-size: var(--nd-text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* A path is the one thing worth breaking across lines instead of truncating. */
  .path {
    white-space: normal;
    overflow-wrap: anywhere;
  }

  .status {
    display: flex;
    align-items: center;
    gap: calc(var(--nd-space) * 1.5);
    font-size: var(--nd-text-xs);
    color: var(--nd-fg-dim);
  }

  .slider {
    /* Own line below the label, so the label can keep its full width. */
    flex-basis: 100%;
    accent-color: var(--nd-accent);
    cursor: pointer;
  }

  .switch {
    position: relative;
    flex: none;
    width: 30px;
    height: 18px;
    padding: 0;
    border: 1px solid var(--nd-border-strong);
    border-radius: 999px;
    background: var(--nd-bg-hover);
    cursor: pointer;
    transition: background var(--nd-duration) var(--nd-ease);
  }

  .switch[aria-checked="true"] {
    background: var(--nd-accent);
    border-color: transparent;
  }

  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    border-radius: 999px;
    background: var(--nd-fg-dim);
    transition:
      transform var(--nd-duration) var(--nd-ease),
      background var(--nd-duration) var(--nd-ease);
  }

  .switch[aria-checked="true"] .knob {
    background: var(--nd-accent-fg);
    transform: translateX(12px);
  }

</style>
