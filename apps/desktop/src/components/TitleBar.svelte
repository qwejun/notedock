<script lang="ts">
  import { Icon, SyncDot, type SyncStatus } from "@notedock/editor";

  interface Props {
    status: SyncStatus;
    pending: number;
    loggedIn: boolean;
    canOpenWeb: boolean;
    onCreate: () => void;
    onSwitch: () => void;
    onOpenWeb: () => void;
    onOpenSettings: () => void;
  }

  let { status, pending, loggedIn, canOpenWeb, onCreate, onSwitch, onOpenWeb, onOpenSettings }: Props = $props();

  const statusLabel = $derived(
    status === "synced" ? "已同步" : status === "syncing" ? "同步中" : "离线",
  );
</script>

<!--
  The whole strip is the drag handle (`data-tauri-drag-region`), which is how an
  undecorated window gets moved. The two controls sit on top of it and stop the
  drag from starting, or clicking one would move the window instead.

  28px tall and carrying exactly three things: the note title, one settings
  button, and the sync dot. Everything else lives behind the settings button —
  in a window that can be 250px wide, a row of six icons is most of the header.
-->
<header data-tauri-drag-region>
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

  {#if loggedIn}
    <div class="actions">
      <button class="nd-btn tool" aria-label="新建笔记" title="新建笔记" onclick={onCreate}>
        <Icon name="plus" size={15} />
      </button>
      <button class="nd-btn tool" aria-label="切换笔记" title="切换笔记" onclick={onSwitch}>
        <Icon name="search" size={15} />
      </button>
      <button class="nd-btn tool" aria-label="设置" title="设置" onclick={onOpenSettings}>
        <Icon name="settings" size={15} />
      </button>
    </div>
  {/if}
</header>

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
    color: var(--nd-fg);
    font-size: var(--nd-text-sm);
    font-weight: 700;
    letter-spacing: 0.01em;
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
  }
</style>
