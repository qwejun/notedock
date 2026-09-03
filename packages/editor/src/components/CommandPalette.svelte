<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { PaletteItem } from "../lib/types";

  interface Props {
    open: boolean;
    items: PaletteItem[];
    onSelect: (id: string) => void;
    onClose: () => void;
    /** Offered as the last row when set, so an empty search can still act. */
    onCreate?: (title: string) => void;
  }

  let { open = $bindable(), items, onSelect, onClose, onCreate }: Props = $props();

  let query = $state("");
  let cursor = $state(0);
  let input = $state<HTMLInputElement>();
  let listbox = $state<HTMLUListElement>();

  /**
   * Plain case-insensitive term matching rather than fuzzy subsequence scoring:
   * these notes are mostly Chinese, where there are no word boundaries or
   * camelCase humps for a fuzzy matcher to latch onto, and skipping characters
   * produces noise instead of matches.
   */
  const matches = $derived.by(() => {
    const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (terms.length === 0) return items;

    return items
      .map((item) => {
        const title = item.title.toLowerCase();
        const body = `${title}\n${item.preview.toLowerCase()}`;
        if (!terms.every((term) => body.includes(term))) return null;
        // Title hits rank above body-only hits.
        const score = terms.filter((term) => title.includes(term)).length;
        return { item, score };
      })
      .filter((hit): hit is { item: PaletteItem; score: number } => hit !== null)
      .sort((a, b) => b.score - a.score)
      .map((hit) => hit.item);
  });

  const canCreate = $derived(Boolean(onCreate));
  /** Index of the "create" row, or -1 when it is not offered. */
  const createIndex = $derived(canCreate ? matches.length : -1);
  const rowCount = $derived(matches.length + (canCreate ? 1 : 0));

  $effect(() => {
    if (!open) return;
    query = "";
    cursor = 0;
    // Focus after the overlay is actually in the DOM.
    requestAnimationFrame(() => input?.focus());
  });

  $effect(() => {
    // Keep the cursor in range as the result set shrinks under the user.
    if (cursor >= rowCount) cursor = Math.max(0, rowCount - 1);
  });

  $effect(() => {
    if (!open) return;
    listbox
      ?.querySelector<HTMLElement>(`[data-index="${cursor}"]`)
      ?.scrollIntoView({ block: "nearest" });
  });

  function commit(): void {
    if (cursor === createIndex) {
      onCreate?.(query.trim());
      onClose();
      return;
    }
    const chosen = matches[cursor];
    if (chosen) {
      onSelect(chosen.id);
      onClose();
    }
  }

  function onKeydown(event: KeyboardEvent): void {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        if (rowCount > 0) cursor = (cursor + 1) % rowCount;
        break;
      case "ArrowUp":
        event.preventDefault();
        if (rowCount > 0) cursor = (cursor - 1 + rowCount) % rowCount;
        break;
      case "Enter":
        event.preventDefault();
        commit();
        break;
      case "Escape":
        event.preventDefault();
        onClose();
        break;
    }
  }
</script>

{#if open}
  <!--
    Dismissal is a backdrop click filtered to the backdrop itself, so the panel
    needs no click handler of its own. The keyboard path is Escape on the input.
  -->
  <div
    class="backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <div
      class="panel nd-glass"
      role="dialog"
      aria-modal="true"
      aria-label="切换笔记"
      tabindex="-1"
    >
      <div class="search">
        <Icon name="search" />
        <input
          bind:this={input}
          bind:value={query}
          class="field"
          type="text"
          placeholder="搜索笔记…"
          role="combobox"
          aria-expanded="true"
          aria-controls="nd-palette-list"
          aria-activedescendant={rowCount > 0 ? `nd-palette-row-${cursor}` : undefined}
          onkeydown={onKeydown}
        />
      </div>

      <ul
        bind:this={listbox}
        id="nd-palette-list"
        class="list"
        role="listbox"
        aria-label="笔记"
      >
        {#each matches as item, index (item.id)}
          <!--
            In a combobox with `aria-activedescendant`, options are not focusable
            and carry no key handlers: all keyboard interaction happens on the
            input above, which owns focus the whole time. The click handler is
            the pointer path only.
          -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <li
            id="nd-palette-row-{index}"
            data-index={index}
            class="row"
            class:current={index === cursor}
            role="option"
            aria-selected={index === cursor}
            onclick={() => {
              onSelect(item.id);
              onClose();
            }}
            onmousemove={() => (cursor = index)}
          >
            <span class="title">{item.title || "未命名"}</span>
            {#if item.preview}<span class="preview">{item.preview}</span>{/if}
          </li>
        {/each}

        {#if canCreate}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <li
            id="nd-palette-row-{createIndex}"
            data-index={createIndex}
            class="row create"
            class:current={cursor === createIndex}
            role="option"
            aria-selected={cursor === createIndex}
            onclick={() => {
              onCreate?.(query.trim());
              onClose();
            }}
            onmousemove={() => (cursor = createIndex)}
          >
            <Icon name="plus" />
            <span class="title">{query.trim() ? `新建「${query.trim()}」` : "新建笔记"}</span>
          </li>
        {/if}

        {#if matches.length === 0 && !canCreate}
          <li class="row empty" role="option" aria-selected="false" aria-disabled="true">
            没有匹配的笔记
          </li>
        {/if}
      </ul>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    /* Sits high rather than centred: in a 200px-tall window a centred dialog
     * has nowhere to grow. */
    padding: calc(var(--nd-space) * 6) calc(var(--nd-space) * 3);
    background: rgb(0 0 0 / 35%);
  }

  .panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    max-width: 420px;
    max-height: min(60vh, 380px);
    border: 1px solid var(--nd-border-strong);
    border-radius: var(--nd-radius-lg);
    box-shadow: var(--nd-shadow-lg);
    overflow: hidden;
  }

  .search {
    display: flex;
    align-items: center;
    gap: calc(var(--nd-space) * 2);
    padding: calc(var(--nd-space) * 2) calc(var(--nd-space) * 3);
    border-bottom: 1px solid var(--nd-border);
    color: var(--nd-fg-faint);
  }

  .field {
    flex: 1;
    min-width: 0;
    border: none;
    background: none;
    color: var(--nd-fg);
    font: inherit;
    font-size: var(--nd-text-md);
  }

  .field:focus {
    outline: none;
  }

  .field::placeholder {
    color: var(--nd-fg-faint);
  }

  .list {
    margin: 0;
    padding: var(--nd-space);
    overflow-y: auto;
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: calc(var(--nd-space) * 2);
    padding: calc(var(--nd-space) * 1.5) calc(var(--nd-space) * 2);
    border-radius: var(--nd-radius-sm);
    cursor: pointer;
  }

  .row.current {
    background: var(--nd-accent-soft);
  }

  .row.empty {
    color: var(--nd-fg-faint);
    cursor: default;
  }

  .title {
    flex: none;
    max-width: 60%;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: var(--nd-text-md);
  }

  .preview {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--nd-fg-faint);
    font-size: var(--nd-text-sm);
  }

  .create {
    color: var(--nd-accent);
  }
</style>
