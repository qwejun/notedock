<script lang="ts">
  import { Icon, type NoteSummary } from "@notedock/editor";

  interface Props {
    notes: NoteSummary[];
    selectedId: string | null;
    onOpen: (id: string) => void;
    onCreate: () => void;
    onDelete: (id: string) => void;
  }

  let { notes, selectedId, onOpen, onCreate, onDelete }: Props = $props();

  let filter = $state("");

  const shown = $derived.by(() => {
    const term = filter.trim().toLowerCase();
    if (!term) return notes;
    return notes.filter(
      (note) =>
        note.title.toLowerCase().includes(term) ||
        note.preview.toLowerCase().includes(term),
    );
  });

  /** Short, unambiguous, and stable regardless of locale width. */
  function when(iso: string): string {
    const then = new Date(iso);
    const minutes = Math.round((Date.now() - then.getTime()) / 60_000);
    if (minutes < 1) return "刚刚";
    if (minutes < 60) return `${minutes} 分钟前`;
    if (minutes < 60 * 24) return `${Math.round(minutes / 60)} 小时前`;
    return then.toLocaleDateString("zh-CN", { month: "numeric", day: "numeric" });
  }
</script>

<aside class="sidebar">
  <header>
    <span class="brand">NoteDock</span>
    <button class="nd-btn" aria-label="新建笔记" onclick={onCreate}>
      <Icon name="plus" />
    </button>
  </header>

  <div class="filter">
    <Icon name="search" size={14} />
    <input
      class="field"
      type="search"
      placeholder="筛选"
      aria-label="筛选笔记"
      bind:value={filter}
    />
  </div>

  <ul>
    {#each shown as note (note.id)}
      <li class:current={note.id === selectedId}>
        <button class="entry" onclick={() => onOpen(note.id)} aria-current={note.id === selectedId}>
          <span class="row">
            <span class="title">{note.title || "未命名"}</span>
            <span class="time">{when(note.updated_at)}</span>
          </span>
          {#if note.preview}<span class="preview">{note.preview}</span>{/if}
        </button>
        <button
          class="nd-btn remove"
          aria-label="删除「{note.title || '未命名'}」"
          onclick={() => onDelete(note.id)}
        >
          <Icon name="trash" size={14} />
        </button>
      </li>
    {/each}

    {#if shown.length === 0}
      <li class="empty">{notes.length === 0 ? "还没有笔记" : "没有匹配的笔记"}</li>
    {/if}
  </ul>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 268px;
    flex: none;
    border-right: 1px solid var(--nd-border);
    background: var(--nd-bg-solid);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 44px;
    padding: 0 calc(var(--nd-space) * 2) 0 calc(var(--nd-space) * 4);
  }

  .brand {
    font-size: var(--nd-text-md);
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .filter {
    display: flex;
    align-items: center;
    gap: calc(var(--nd-space) * 2);
    margin: 0 calc(var(--nd-space) * 2) var(--nd-space);
    padding: 0 calc(var(--nd-space) * 2);
    height: 28px;
    border-radius: var(--nd-radius-sm);
    background: var(--nd-bg-hover);
    color: var(--nd-fg-faint);
  }

  .field {
    flex: 1;
    min-width: 0;
    border: none;
    background: none;
    color: var(--nd-fg);
    font: inherit;
    font-size: var(--nd-text-sm);
  }

  .field:focus {
    outline: none;
  }

  .field::-webkit-search-cancel-button {
    display: none;
  }

  ul {
    flex: 1;
    margin: 0;
    padding: 0 var(--nd-space) calc(var(--nd-space) * 2);
    overflow-y: auto;
    list-style: none;
  }

  li {
    position: relative;
    border-radius: var(--nd-radius-sm);
  }

  li:hover {
    background: var(--nd-bg-hover);
  }

  li.current {
    background: var(--nd-accent-soft);
  }

  .entry {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: calc(var(--nd-space) * 2) calc(var(--nd-space) * 2.5);
    border: none;
    border-radius: inherit;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .row {
    display: flex;
    align-items: baseline;
    gap: calc(var(--nd-space) * 2);
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: var(--nd-text-md);
  }

  .time {
    flex: none;
    color: var(--nd-fg-faint);
    font-size: var(--nd-text-xs);
    /* Yields room to the delete button, which overlays this corner on hover. */
    padding-right: 18px;
  }

  .preview {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--nd-fg-faint);
    font-size: var(--nd-text-sm);
  }

  /* Hidden until hover or keyboard focus, so the list stays quiet. */
  .remove {
    position: absolute;
    top: calc(var(--nd-space) * 1.5);
    right: var(--nd-space);
    opacity: 0;
    transition: opacity var(--nd-duration) var(--nd-ease);
  }

  li:hover .remove,
  .remove:focus-visible {
    opacity: 1;
  }

  .remove:hover {
    color: var(--nd-danger);
  }

  .empty {
    padding: calc(var(--nd-space) * 3) calc(var(--nd-space) * 2.5);
    color: var(--nd-fg-faint);
    font-size: var(--nd-text-sm);
  }
</style>
