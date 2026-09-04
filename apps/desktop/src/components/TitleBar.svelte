<script lang="ts">
  import { Icon, SyncDot, type SyncStatus } from "@notedock/editor";

  interface Props {
    status: SyncStatus;
    pending: number;
    loggedIn: boolean;
    canOpenWeb: boolean;
    maximized: boolean;
    onCreate: () => void;
    onSwitch: () => void;
    onOpenWeb: () => void;
    onOpenSettings: () => void;
    onToggleMaximize: () => void;
    onMinimize: () => void;
    onHide: () => void;
    onQuit: () => void;
  }

  let {
    status,
    pending,
    loggedIn,
    canOpenWeb,
    maximized,
    onCreate,
    onSwitch,
    onOpenWeb,
    onOpenSettings,
    onToggleMaximize,
    onMinimize,
    onHide,
    onQuit,
  }: Props = $props();

  let contextMenu = $state<{ x: number; y: number } | null>(null);

  const statusLabel = $derived(
    status === "synced" ? "已同步" : status === "syncing" ? "同步中" : "离线",
  );

  /** One label for the button, its tooltip and the menu item. */
  const sizeLabel = $derived(maximized ? "还原窗口" : "全屏");

  function openContextMenu(event: MouseEvent): void {
    event.preventDefault();
    const menuWidth = 148;
    const menuHeight = 156;
    contextMenu = {
      x: Math.min(event.clientX, Math.max(4, window.innerWidth - menuWidth - 4)),
      y: Math.min(event.clientY, Math.max(4, window.innerHeight - menuHeight - 4)),
    };
  }

  function closeContextMenu(): void {
    contextMenu = null;
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") closeContextMenu();
  }
</script>

<svelte:window onclick={closeContextMenu} onkeydown={handleKeydown} />

<!--
  The whole strip is the drag handle (`data-tauri-drag-region`), which is how an
  undecorated window gets moved. The controls sit on top of it and stop the drag
  from starting, or clicking one would move the window instead.

  Two groups, and the split matters: the app's own tools on the left of the pair,
  then the caption trio Windows would have drawn if the window had a frame. The
  tools hide until login; the caption buttons never do, because a window you
  cannot minimise or dismiss is a trap. At 250px the brand ellipsises to make room
  rather than the buttons wrapping.

  × dismisses to the tray rather than quitting — see `hide_window` in commands.rs.
  Quitting lives in the right-click menu, 设置, and the tray.
-->
<header role="toolbar" tabindex="-1" data-tauri-drag-region oncontextmenu={openContextMenu}>
  <div class="identity" data-tauri-drag-region>
    {#if loggedIn && canOpenWeb}
      <button class="brand-link" aria-label="打开 Web 端" title="打开 Web 端" onclick={onOpenWeb}>NoteDock</button>
    {:else}
      <span class="brand" data-tauri-drag-region>NoteDock</span>
    {/if}
    {#if loggedIn}
      <span class:online={status === "synced"} class:pending={status === "syncing"} class="status" title={pending > 0 ? `${pending} 处改动待同步` : statusLabel}>
        <SyncDot {status} />
      </span>
    {/if}
  </div>

  <div class="actions">
    {#if loggedIn}
      <button class="nd-btn tool" aria-label="新建笔记" title="新建笔记" onclick={onCreate}>
        <Icon name="plus" size={15} />
      </button>
      <button class="nd-btn tool" aria-label="切换笔记" title="切换笔记" onclick={onSwitch}>
        <Icon name="search" size={15} />
      </button>
      <button class="nd-btn tool" aria-label="设置" title="设置" onclick={onOpenSettings}>
        <Icon name="settings" size={15} />
      </button>
    {/if}
    <div class="caption">
      <button class="nd-btn cap" aria-label="最小化" title="最小化" onclick={onMinimize}>
        <Icon name="minimize" size={14} />
      </button>
      <button
        class="nd-btn cap"
        aria-label={sizeLabel}
        title={`${sizeLabel} · F11`}
        onclick={onToggleMaximize}
      >
        <Icon name={maximized ? "restore" : "maximize"} size={13} />
      </button>
      <button
        class="nd-btn cap dismiss"
        aria-label="隐藏窗口"
        title="隐藏到托盘 · 点托盘图标恢复"
        onclick={onHide}
      >
        <Icon name="close" size={14} />
      </button>
    </div>
  </div>
</header>

{#if contextMenu}
  <div
    class="context-menu"
    role="menu"
    tabindex="-1"
    style={`left: ${contextMenu.x}px; top: ${contextMenu.y}px;`}
  >
    <button role="menuitem" onclick={() => { closeContextMenu(); onToggleMaximize(); }}>
      <Icon name={maximized ? "restore" : "maximize"} size={15} />
      <span>{sizeLabel}</span>
    </button>
    <button role="menuitem" onclick={() => { closeContextMenu(); onMinimize(); }}>
      <Icon name="minimize" size={15} />
      <span>最小化</span>
    </button>
    <button role="menuitem" onclick={() => { closeContextMenu(); onHide(); }}>
      <Icon name="close" size={15} />
      <span>隐藏窗口</span>
    </button>
    <!-- 退出 is the only irreversible item here, so it gets a rule above it and the
         one bit of colour: the three above it can all be undone by clicking again. -->
    <hr />
    <button class="danger" role="menuitem" onclick={() => { closeContextMenu(); onQuit(); }}>
      <Icon name="power" size={15} />
      <span>退出 NoteDock</span>
    </button>
  </div>
{/if}

<style>
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex: none;
    height: 48px;
    padding: 0 calc(var(--nd-space) * 4);
    border-bottom: 1px solid var(--nd-border);
    background: rgb(255 255 255 / 2%);
    transition:
      border-color var(--nd-duration) var(--nd-ease),
      background var(--nd-duration) var(--nd-ease);
  }

  header:hover,
  header:focus-within {
    background: var(--nd-bg-hover);
  }

  .identity,
  .actions {
    display: flex;
    align-items: center;
  }

  .identity {
    min-width: 0;
    gap: calc(var(--nd-space) * 2);
    flex: 1;
  }

  .brand,
  .brand-link {
    overflow: hidden;
    color: var(--nd-fg);
    font-size: var(--nd-text-sm);
    font-weight: 700;
    letter-spacing: 0.01em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .brand-link {
    padding: 0;
    border: 0;
    background: transparent;
    font: inherit;
    cursor: pointer;
  }

  .brand-link:hover {
    color: var(--nd-accent);
  }

  .actions {
    flex: none;
    gap: calc(var(--nd-space) * 1.5);
  }

  .status {
    display: inline-flex;
    align-items: center;
    gap: calc(var(--nd-space) * 1.5);
    flex: none;
    min-height: 24px;
    padding: 0 4px 0 2px;
    color: var(--nd-fg-faint);
    font-size: var(--nd-text-xs);
  }

  .status.online {
    color: #7bd99a;
  }

  .status.pending {
    color: #e8b85e;
  }

  .tool {
    flex: none;
    height: 30px;
    min-width: 30px;
    color: var(--nd-fg-dim);
  }

  /* Flush against each other and set off by a rule: the caption trio is one
     object borrowed from the OS, not three more app tools. */
  .caption {
    display: flex;
    align-items: center;
    margin-left: calc(var(--nd-space) * 1.5);
    padding-left: calc(var(--nd-space) * 1.5);
    border-left: 1px solid var(--nd-border);
  }

  .cap {
    flex: none;
    height: 28px;
    min-width: 26px;
    padding: 0;
    color: var(--nd-fg-dim);
  }

  /* The only colour in the strip, and only on hover: a permanently red × in a
     window that floats over everything reads as an error state. Red on hover even
     though this one only hides — every tray-resident Windows app does the same, and
     it marks the button as the × rather than promising to end anything. Specific
     enough to beat `.nd-btn:hover` from base.css whichever order the two land in. */
  .cap.dismiss:hover:not(:disabled) {
    background: var(--nd-danger);
    color: #fff;
  }

  .context-menu {
    position: fixed;
    z-index: 30;
    display: grid;
    min-width: 132px;
    padding: 4px;
    border: 1px solid var(--nd-border-strong);
    border-radius: var(--nd-radius-sm);
    background: var(--nd-bg-elev);
    box-shadow: var(--nd-shadow-lg);
  }

  .context-menu button {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    min-height: 34px;
    padding: 0 9px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--nd-fg);
    font: inherit;
    font-size: var(--nd-text-xs);
    text-align: left;
    cursor: pointer;
  }

  .context-menu button:hover,
  .context-menu button:focus-visible {
    background: var(--nd-bg-hover);
    outline: none;
  }

  .context-menu button.danger { color: var(--nd-danger); }

  .context-menu hr {
    height: 0;
    margin: 4px 5px;
    border: 0;
    border-top: 1px solid var(--nd-border);
  }

  @media (max-width: 320px) {
    .status {
      padding: 0;
    }

    header {
      padding-inline: calc(var(--nd-space) * 2);
    }

    .identity {
      gap: var(--nd-space);
    }

    /* Six buttons in 250px: the tools give up their padding so the caption trio
       keeps a full-size hit target. */
    .tool {
      min-width: 26px;
      padding: 0;
    }

    .caption {
      margin-left: var(--nd-space);
      padding-left: var(--nd-space);
    }
  }
</style>
