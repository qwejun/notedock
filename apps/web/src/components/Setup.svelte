<script lang="ts">
  interface Props {
    busy: boolean;
    message: string | null;
    onSubmit: (password: string) => void;
  }

  let { busy, message, onSubmit }: Props = $props();
  let password = $state("");
  let confirm = $state("");

  let mismatch = $derived(Boolean(confirm) && password !== confirm);
  let tooShort = $derived(Boolean(password) && password.length < 8);
</script>

<div class="wrap">
  <form
    class="card nd-glass"
    onsubmit={(event) => {
      event.preventDefault();
      if (password.length >= 8 && password === confirm) onSubmit(password);
    }}
  >
    <h1>设置 NoteDock</h1>
    <p class="hint">这是第一次打开，先设置一个访问密码。</p>

    <label for="nd-setup-password">访问密码</label>
    <input
      id="nd-setup-password"
      class="nd-input"
      type="password"
      autocomplete="new-password"
      placeholder="至少 8 个字符"
      bind:value={password}
      disabled={busy}
    />

    <label for="nd-setup-confirm">再输入一次</label>
    <input
      id="nd-setup-confirm"
      class="nd-input"
      type="password"
      autocomplete="new-password"
      placeholder="确认访问密码"
      bind:value={confirm}
      disabled={busy}
    />

    {#if tooShort}
      <p class="error">密码至少需要 8 个字符</p>
    {:else if mismatch}
      <p class="error">两次输入的密码不一致</p>
    {/if}

    <button
      class="nd-btn nd-btn--primary submit"
      type="submit"
      disabled={busy || password.length < 8 || password !== confirm}
    >
      {busy ? "保存中…" : "保存并进入"}
    </button>

    {#if message}
      <p class="error" role="alert">{message}</p>
    {/if}
  </form>
</div>

<style>
  .wrap {
    display: grid;
    place-items: center;
    height: 100%;
    padding: calc(var(--nd-space) * 4);
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: calc(var(--nd-space) * 2);
    width: 100%;
    max-width: 320px;
    padding: calc(var(--nd-space) * 7);
    border: 1px solid var(--nd-border);
    border-radius: var(--nd-radius-lg);
    box-shadow: var(--nd-shadow-md);
  }

  h1 { margin: 0; font-size: var(--nd-text-lg); font-weight: 600; }
  .hint { margin: 0 0 calc(var(--nd-space) * 2); color: var(--nd-fg-dim); font-size: var(--nd-text-sm); }
  label { margin-top: var(--nd-space); color: var(--nd-fg-dim); font-size: var(--nd-text-xs); }
  .submit { height: 32px; justify-content: center; margin-top: var(--nd-space); }
  .error { margin: 0; color: var(--nd-danger); font-size: var(--nd-text-sm); }
</style>
