<script lang="ts">
  import type { SyncStatus } from "../lib/types";

  interface Props {
    status: SyncStatus;
    /** Show the label next to the dot. Off in the floating window, on in the browser. */
    withLabel?: boolean;
  }

  let { status, withLabel = false }: Props = $props();

  const LABELS: Record<SyncStatus, string> = {
    synced: "已同步",
    syncing: "同步中",
    offline: "离线",
  };

  const label = $derived(LABELS[status]);
</script>

<!--
  Status is a 6px dot, not a sentence: in the floating window it has to coexist
  with the note in 250px of width. The accessible name carries the full text, and
  `title` surfaces it on hover for sighted users.
-->
<span class="wrap" title={label}>
  <span class="dot" data-status={status} role="img" aria-label="同步状态：{label}"></span>
  {#if withLabel}<span class="text">{label}</span>{/if}
</span>

<style>
  .wrap {
    display: inline-flex;
    align-items: center;
    gap: calc(var(--nd-space) * 1.5);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: var(--nd-fg-faint);
  }

  .dot[data-status="synced"] {
    background: var(--nd-ok);
  }

  .dot[data-status="syncing"] {
    background: var(--nd-accent);
    animation: pulse 1.4s ease-in-out infinite;
  }

  .text {
    color: var(--nd-fg-dim);
    font-size: var(--nd-text-xs);
  }

  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dot[data-status="syncing"] {
      animation: none;
    }
  }
</style>
