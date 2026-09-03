<script lang="ts">
  import type { Editor } from "@tiptap/core";
  import Icon from "./Icon.svelte";
  import { HEADING_LEVELS, HIGHLIGHT_COLORS, TEXT_COLORS } from "../lib/tiptap";

  interface Props {
    editor: Editor;
    /** Positioning frame. The bar is clamped inside it, never overflowing. */
    container: HTMLElement | undefined;
  }

  let { editor, container }: Props = $props();

  /** Distance kept from the selection and from the container edges. */
  const GAP = 8;
  const MARGIN = 4;

  type Panel = "none" | "color" | "highlight" | "link";

  let bar = $state<HTMLDivElement>();
  let panel = $state<Panel>("none");
  let linkDraft = $state("");
  let left = $state(0);
  let top = $state(0);
  let visible = $state(false);

  /**
   * Bumped on every editor transaction. Reading it inside {@link active} is what
   * makes button states reactive — TipTap's editor is not a Svelte store.
   */
  let tick = $state(0);

  function active(name: string, attrs?: Record<string, unknown>): boolean {
    return tick >= 0 && editor.isActive(name, attrs);
  }

  function clamp(value: number, min: number, max: number): number {
    return Math.max(min, Math.min(value, Math.max(min, max)));
  }

  function reposition(): void {
    const { selection } = editor.state;
    // Visibility follows the selection alone. Focus is unreliable here: clicking
    // a swatch moves focus to the button, and the bar must not vanish mid-choice.
    visible = editor.isEditable && !selection.empty;
    if (!visible) {
      panel = "none";
      return;
    }
    if (!bar || !container) return;

    const view = editor.view;
    const start = view.coordsAtPos(selection.from);
    const end = view.coordsAtPos(selection.to);
    const frame = container.getBoundingClientRect();
    const size = bar.getBoundingClientRect();

    const centerX =
      (Math.min(start.left, end.left) + Math.max(start.right, end.right)) / 2;
    left = clamp(
      centerX - frame.left - size.width / 2,
      MARGIN,
      frame.width - size.width - MARGIN,
    );

    // Prefer above the selection; fall back below when there is no room, which
    // is most of the time in a 200px-tall floating window.
    const above = Math.min(start.top, end.top) - frame.top - size.height - GAP;
    const below = Math.max(start.bottom, end.bottom) - frame.top + GAP;
    top = clamp(
      above >= MARGIN ? above : below,
      MARGIN,
      frame.height - size.height - MARGIN,
    );
  }

  $effect(() => {
    const onChange = () => {
      tick += 1;
      reposition();
    };
    editor.on("transaction", onChange);
    editor.on("selectionUpdate", onChange);
    return () => {
      editor.off("transaction", onChange);
      editor.off("selectionUpdate", onChange);
    };
  });

  // The selection can stay put while the text under it moves. `capture` is
  // needed because scroll does not bubble — the scrolling element is a
  // descendant of the positioning frame, not the frame itself.
  $effect(() => {
    if (!container) return;
    const onScroll = () => reposition();
    container.addEventListener("scroll", onScroll, {
      capture: true,
      passive: true,
    });
    window.addEventListener("resize", onScroll);
    return () => {
      container.removeEventListener("scroll", onScroll, { capture: true });
      window.removeEventListener("resize", onScroll);
    };
  });

  /** Re-measure once the bar has its final size, e.g. after a panel opens. */
  $effect(() => {
    panel;
    if (visible) requestAnimationFrame(reposition);
  });

  function run(action: (chain: ReturnType<Editor["chain"]>) => void): void {
    const chain = editor.chain().focus();
    action(chain);
    panel = "none";
  }

  function togglePanel(next: Panel): void {
    panel = panel === next ? "none" : next;
    if (next === "link") {
      linkDraft = editor.getAttributes("link").href ?? "";
    }
  }

  function applyLink(): void {
    const href = linkDraft.trim();
    if (!href) {
      run((chain) => chain.unsetLink().run());
      return;
    }
    // Bare domains typed by hand would otherwise resolve relative to the app.
    const url = /^(https?:|mailto:)/i.test(href) ? href : `https://${href}`;
    run((chain) => chain.setLink({ href: url }).run());
  }
</script>

<!--
  `onmousedown|preventDefault` on every control is load-bearing: without it the
  browser collapses the selection before the click lands and the command applies
  to nothing.
-->
<div
  bind:this={bar}
  class="bubble nd-glass"
  class:visible
  style:left="{left}px"
  style:top="{top}px"
  role="toolbar"
  aria-label="文字格式"
  aria-hidden={!visible}
  onmousedown={(event) => event.preventDefault()}
>
  <div class="row">
    <button
      class="nd-btn mark"
      aria-label="粗体"
      aria-pressed={active("bold")}
      onclick={() => run((c) => c.toggleBold().run())}><b>B</b></button
    >
    <button
      class="nd-btn mark"
      aria-label="斜体"
      aria-pressed={active("italic")}
      onclick={() => run((c) => c.toggleItalic().run())}><i>I</i></button
    >
    <button
      class="nd-btn mark"
      aria-label="下划线"
      aria-pressed={active("underline")}
      onclick={() => run((c) => c.toggleUnderline().run())}><u>U</u></button
    >
    <button
      class="nd-btn mark"
      aria-label="删除线"
      aria-pressed={active("strike")}
      onclick={() => run((c) => c.toggleStrike().run())}><s>S</s></button
    >
    <button
      class="nd-btn mark mono"
      aria-label="行内代码"
      aria-pressed={active("code")}
      onclick={() => run((c) => c.toggleCode().run())}>{"</>"}</button
    >

    <span class="sep" role="separator"></span>

    {#each HEADING_LEVELS as level (level)}
      <button
        class="nd-btn mark"
        aria-label="{level} 级标题"
        aria-pressed={active("heading", { level })}
        onclick={() => run((c) => c.toggleHeading({ level }).run())}
        >H{level}</button
      >
    {/each}

    <span class="sep" role="separator"></span>

    <button
      class="nd-btn"
      aria-label="项目符号列表"
      aria-pressed={active("bulletList")}
      onclick={() => run((c) => c.toggleBulletList().run())}
      ><Icon name="bulletList" /></button
    >
    <button
      class="nd-btn"
      aria-label="编号列表"
      aria-pressed={active("orderedList")}
      onclick={() => run((c) => c.toggleOrderedList().run())}
      ><Icon name="orderedList" /></button
    >
    <button
      class="nd-btn"
      aria-label="引用"
      aria-pressed={active("blockquote")}
      onclick={() => run((c) => c.toggleBlockquote().run())}
      ><Icon name="quote" /></button
    >

    <span class="sep" role="separator"></span>

    <button
      class="nd-btn"
      aria-label="文字颜色"
      aria-expanded={panel === "color"}
      onclick={() => togglePanel("color")}><Icon name="textColor" /></button
    >
    <button
      class="nd-btn"
      aria-label="高亮"
      aria-expanded={panel === "highlight"}
      onclick={() => togglePanel("highlight")}><Icon name="highlight" /></button
    >
    <button
      class="nd-btn"
      aria-label="链接"
      aria-expanded={panel === "link"}
      aria-pressed={active("link")}
      onclick={() => togglePanel("link")}><Icon name="link" /></button
    >
    <button
      class="nd-btn"
      aria-label="清除格式"
      onclick={() => run((c) => c.unsetAllMarks().clearNodes().run())}
      ><Icon name="clear" /></button
    >
  </div>

  {#if panel === "color" || panel === "highlight"}
    {@const swatches = panel === "color" ? TEXT_COLORS : HIGHLIGHT_COLORS}
    <div class="row swatches" role="group" aria-label={panel === "color" ? "文字颜色" : "高亮颜色"}>
      {#each swatches as swatch (swatch.value)}
        <button
          class="swatch"
          style:background={swatch.value}
          aria-label={swatch.label}
          onclick={() =>
            run((c) =>
              panel === "color"
                ? c.setColor(swatch.value).run()
                : c.toggleHighlight({ color: swatch.value }).run(),
            )}
        ></button>
      {/each}
      <button
        class="nd-btn reset"
        onclick={() =>
          run((c) => (panel === "color" ? c.unsetColor().run() : c.unsetHighlight().run()))}
        >无</button
      >
    </div>
  {/if}

  {#if panel === "link"}
    <div class="row">
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="nd-input link-input"
        type="url"
        placeholder="https://…"
        aria-label="链接地址"
        autofocus
        bind:value={linkDraft}
        onmousedown={(event) => event.stopPropagation()}
        onkeydown={(event) => {
          if (event.key === "Enter") applyLink();
          if (event.key === "Escape") panel = "none";
        }}
      />
      <button class="nd-btn" aria-label="应用链接" onclick={applyLink}
        ><Icon name="check" /></button
      >
    </div>
  {/if}
</div>

<style>
  .bubble {
    position: absolute;
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: var(--nd-space);
    /* Never wider than the frame: this is what keeps the bar inside a 250px
     * window instead of clipping off the right edge. */
    max-width: calc(100% - var(--nd-space) * 2);
    padding: var(--nd-space);
    border: 1px solid var(--nd-border-strong);
    border-radius: var(--nd-radius-md);
    box-shadow: var(--nd-shadow-md);
    opacity: 0;
    /* `visibility: hidden` and not just `opacity: 0`: a transparent bar would
     * still be in the tab order, so Tab from the editor would walk through a
     * dozen invisible buttons. */
    visibility: hidden;
    transform: translateY(2px) scale(0.98);
    pointer-events: none;
    transition:
      opacity var(--nd-duration) var(--nd-ease),
      transform var(--nd-duration) var(--nd-ease),
      visibility var(--nd-duration);
  }

  .bubble.visible {
    opacity: 1;
    visibility: visible;
    transform: none;
    pointer-events: auto;
  }

  .row {
    display: flex;
    /* Wrapping beats a horizontal scroller: every control stays reachable by
     * both mouse and Tab at any window width. */
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
  }

  .sep {
    width: 1px;
    height: 16px;
    margin: 0 2px;
    background: var(--nd-border-strong);
  }

  .mark {
    min-width: 24px;
    padding: 0 calc(var(--nd-space) * 1.25);
    font-size: var(--nd-text-sm);
    font-weight: 600;
  }

  .mark.mono {
    font-family: var(--nd-font-mono);
    font-size: var(--nd-text-xs);
  }

  .swatches {
    padding-top: 2px;
    border-top: 1px solid var(--nd-border);
  }

  .swatch {
    width: 20px;
    height: 20px;
    border: 1px solid var(--nd-border-strong);
    border-radius: 999px;
    cursor: pointer;
    transition: transform var(--nd-duration) var(--nd-ease);
  }

  .swatch:hover {
    transform: scale(1.15);
  }

  .reset {
    min-width: 24px;
    font-size: var(--nd-text-xs);
  }

  .link-input {
    width: 168px;
    height: 26px;
    font-size: var(--nd-text-sm);
  }
</style>
