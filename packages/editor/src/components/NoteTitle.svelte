<script lang="ts">
  /**
   * The note's title field.
   *
   * Shared rather than written once per app because it is not a plain controlled
   * input: the value it shows is collaborative, so it can change while being
   * typed into, and the two failure modes that causes are worth fixing in one
   * place.
   */
  interface Props {
    /** Current title, which may change from another device mid-edit. */
    value: string;
    placeholder?: string;
    /** Called with the whole field, never mid-composition. */
    onInput: (value: string) => void;
  }

  let { value, placeholder = "标题", onInput }: Props = $props();

  let input = $state<HTMLInputElement>();

  /**
   * True between `compositionstart` and `compositionend`.
   *
   * Not `$state`: it exists to be read by the effect below as a guard, and making
   * it reactive would re-run that effect on every composition boundary for no
   * benefit.
   */
  let composing = false;

  /*
   * Pushes a remote rename into the field without disturbing what is being typed.
   *
   * A bare `value={...}` cannot do this. Every assignment to `input.value` drops
   * the caret to the end, so a rename arriving from another device — or the note's
   * own title arriving over the socket — would yank the cursor mid-word. Worse,
   * an assignment landing inside an IME composition leaves the pending characters
   * in the composition buffer, and committing then inserts them a second time.
   *
   * So: only correct the DOM when it genuinely disagrees, never while a
   * composition is open, and put the caret back where it was.
   */
  $effect(() => {
    const element = input;
    const next = value;
    if (!element || composing || element.value === next) return;

    const caret = element.selectionStart;
    element.value = next;
    if (caret !== null && document.activeElement === element) {
      const at = Math.min(caret, next.length);
      element.setSelectionRange(at, at);
    }
  });
</script>

<input
  bind:this={input}
  class="nd-note-title"
  type="text"
  aria-label="笔记标题"
  {placeholder}
  maxlength="200"
  autocomplete="off"
  spellcheck="false"
  oninput={(event) => {
    // Intermediate composition text is not a title. Waiting for the commit also
    // keeps candidate strings off the wire and out of everyone else's list.
    if (!composing) onInput(event.currentTarget.value);
  }}
  oncompositionstart={() => (composing = true)}
  oncompositionend={(event) => {
    composing = false;
    onInput(event.currentTarget.value);
  }}
/>

<style>
  /*
   * Structural only. The two apps want visibly different headers — a 72px slab in
   * the floating window, a tighter one on the web — so each styles
   * `.nd-note-title` itself, the same way it styles the editor's `.frame`.
   */
  .nd-note-title {
    flex: none;
    width: 100%;
    border: 0;
    background: transparent;
    color: var(--nd-fg);
    font: inherit;
    outline: none;
  }

  .nd-note-title::placeholder {
    color: var(--nd-fg-faint);
  }
</style>
