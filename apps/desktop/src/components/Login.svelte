<script lang="ts">
  import { untrack } from "svelte";

  interface Props {
    busy: boolean;
    message: string | null;
    /** Seed only: the last server this app talked to, so a re-login is one keystroke. */
    initialUrl: string;
    onSubmit: (serverUrl: string, password: string) => void;
  }

  let { busy, message, initialUrl, onSubmit }: Props = $props();

  // `untrack` says what is meant: this is the field's starting value, and later
  // changes to the prop must not overwrite what the user is typing.
  let serverUrl = $state(untrack(() => initialUrl));
  let password = $state("");
</script>

<form
  class="panel"
  onsubmit={(event) => {
    event.preventDefault();
    if (serverUrl && password) onSubmit(serverUrl, password);
  }}
>
  <p class="lead">登录 NoteDock</p>

  <label for="nd-server">服务器地址或 IP</label>
  <input
    id="nd-server"
    class="nd-input"
    type="text"
    inputmode="url"
    placeholder="http://192.168.1.10:8080"
    aria-label="服务器地址"
    autocomplete="off"
    spellcheck="false"
    bind:value={serverUrl}
    disabled={busy}
  />
  <label for="nd-password">密码</label>
  <input
    id="nd-password"
    class="nd-input"
    type="password"
    placeholder="密码"
    aria-label="访问密码"
    autocomplete="current-password"
    bind:value={password}
    disabled={busy}
  />

  <button
    class="nd-btn nd-btn--primary submit"
    type="submit"
    disabled={busy || !serverUrl || !password}
  >
    {busy ? "登录中…" : "登录"}
  </button>

  {#if message}
    <p class="error" role="alert">{message}</p>
  {/if}

  <p class="note">
    没有 https 时连接不加密。笔记会先存在本机，联网后再上传。
  </p>
</form>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: calc(var(--nd-space) * 2);
    /* Scrolls rather than clips: the window can be as short as 200px. */
    overflow-y: auto;
    padding: calc(var(--nd-space) * 4);
  }

  .lead {
    margin: 0;
    font-size: var(--nd-text-sm);
    font-weight: 600;
  }

  label {
    margin-bottom: calc(var(--nd-space) * -1);
    color: var(--nd-fg-dim);
    font-size: var(--nd-text-xs);
  }

  .submit {
    height: 30px;
    justify-content: center;
  }

  .error {
    margin: 0;
    color: var(--nd-danger);
    font-size: var(--nd-text-xs);
  }

  .note {
    margin: 0;
    color: var(--nd-fg-faint);
    font-size: var(--nd-text-xs);
    line-height: 1.5;
  }
</style>
