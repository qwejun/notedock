<script lang="ts">
  interface Props {
    busy: boolean;
    message: string | null;
    onSubmit: (password: string) => void;
  }

  let { busy, message, onSubmit }: Props = $props();

  let password = $state("");

  /**
   * The first-stage deployment is plain HTTP over a public address, so say so
   * rather than letting the padlock's absence go unnoticed.
   */
  const insecure =
    typeof location !== "undefined" &&
    location.protocol !== "https:" &&
    location.hostname !== "localhost" &&
    location.hostname !== "127.0.0.1";
</script>

<div class="wrap">
  <form
    class="card nd-glass"
    onsubmit={(event) => {
      event.preventDefault();
      if (password) onSubmit(password);
    }}
  >
    <h1>NoteDock</h1>
    <p class="hint">输入访问密码以打开你的笔记</p>

    <input
      class="nd-input"
      type="password"
      autocomplete="current-password"
      placeholder="密码"
      aria-label="访问密码"
      bind:value={password}
      disabled={busy}
    />

    <button class="nd-btn nd-btn--primary submit" type="submit" disabled={busy || !password}>
      {busy ? "登录中…" : "登录"}
    </button>

    {#if message}
      <p class="error" role="alert">{message}</p>
    {/if}

    {#if insecure}
      <p class="warn">
        当前是 HTTP 连接，密码和笔记内容不会加密传输。同一网络上的人可以看到。
      </p>
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
    gap: calc(var(--nd-space) * 3);
    width: 100%;
    max-width: 320px;
    padding: calc(var(--nd-space) * 7);
    border: 1px solid var(--nd-border);
    border-radius: var(--nd-radius-lg);
    box-shadow: var(--nd-shadow-md);
  }

  h1 {
    margin: 0;
    font-size: var(--nd-text-lg);
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .hint {
    margin: calc(var(--nd-space) * -2) 0 0;
    color: var(--nd-fg-dim);
    font-size: var(--nd-text-sm);
  }

  .submit {
    height: 32px;
    justify-content: center;
  }

  .error {
    margin: 0;
    color: var(--nd-danger);
    font-size: var(--nd-text-sm);
  }

  .warn {
    margin: 0;
    padding-top: calc(var(--nd-space) * 2);
    border-top: 1px solid var(--nd-border);
    color: var(--nd-warn);
    font-size: var(--nd-text-xs);
    line-height: 1.6;
  }
</style>
