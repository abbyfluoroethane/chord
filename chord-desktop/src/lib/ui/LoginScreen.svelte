<script lang="ts">
  // Sign-in: address, password, remember me, optional server.
  import ChevronDown from 'lucide-svelte/icons/chevron-down';
  import CircleAlert from 'lucide-svelte/icons/circle-alert';
  import Icon from './Icon.svelte';
  import RegisterForm from './RegisterForm.svelte';
  import { session } from './session.svelte';

  let address = $state(session.savedAddress());
  let password = $state('');
  let remember = $state(session.savedAddress() !== '');
  let server = $state('');
  let advanced = $state(false);
  let registering = $state(false);

  const busy = $derived(session.state === 'connecting');

  /** The first empty field gets the focus: the address, or the password when the address is saved. */
  function focusStart(form: HTMLFormElement) {
    const target = form.querySelector<HTMLInputElement>(address.trim() ? '#password' : '#address');
    target?.focus();
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    void session.signIn({ address, password, remember, server });
  }

  /** A new account: sign in with it at once. */
  function registered(newAddress: string, newPassword: string) {
    registering = false;
    if (newAddress) address = newAddress;
    if (newAddress && newPassword) {
      password = newPassword;
      void session.signIn({ address: newAddress, password: newPassword, remember, server });
    }
  }
</script>

<main class="login">
  {#if registering}
    <div class="card">
      <RegisterForm {address} {server} onback={() => (registering = false)} onregistered={registered} />
    </div>
  {:else}
  <form use:focusStart onsubmit={submit} aria-labelledby="login-title" novalidate>
    <div class="brand">
      <svg width="40" height="40" viewBox="0 0 40 40" fill="none" aria-hidden="true">
        <circle cx="20" cy="20" r="16" stroke="var(--ink)" stroke-width="3" />
        <line x1="9" y1="27" x2="31" y2="13" stroke="var(--brand)" stroke-width="3" stroke-linecap="round" />
        <circle cx="9" cy="27" r="3.5" fill="var(--brand)" />
        <circle cx="31" cy="13" r="3.5" fill="var(--brand)" />
      </svg>
      <span class="word">chord</span>
    </div>
    <h1 id="login-title" class="title">Sign in</h1>

    <div class="field">
      <label for="address">Address</label>
      <input
        id="address"
        class="input mono"
        type="text"
        inputmode="email"
        autocomplete="username"
        autocapitalize="off"
        spellcheck="false"
        placeholder="you@example.com"
        bind:value={address}
        disabled={busy}
        aria-invalid={!!session.error}
        aria-describedby={session.error ? 'login-error' : undefined}
      />
    </div>

    <div class="field">
      <label for="password">Password</label>
      <input
        id="password"
        class="input"
        type="password"
        autocomplete="current-password"
        bind:value={password}
        disabled={busy}
        aria-invalid={!!session.error}
      />
    </div>

    <label class="check"><input type="checkbox" bind:checked={remember} disabled={busy} /> Remember me</label>

    <button
      type="button"
      class="adv"
      aria-expanded={advanced}
      aria-controls="server-field"
      onclick={() => (advanced = !advanced)}
    >
      <span class:open={advanced}><Icon icon={ChevronDown} size={16} /></span> Advanced
    </button>
    {#if advanced}
      <div class="field" id="server-field">
        <label for="server">Server (optional)</label>
        <input
          id="server"
          class="input mono"
          placeholder="chat.example.com:5222"
          autocapitalize="off"
          spellcheck="false"
          bind:value={server}
          disabled={busy}
        />
        <span class="meta">Leave this empty. Chord finds your server from your address.</span>
      </div>
    {/if}

    {#if session.error}
      <p id="login-error" class="error" role="alert">
        <Icon icon={CircleAlert} size={16} />
        {session.error}
      </p>
    {/if}

    {#if session.certProblem}
      {@const cert = session.certProblem}
      {@const host = address.split('@')[1] || 'the server'}
      <div class="cert" role="alertdialog" aria-labelledby="cert-title">
        <strong id="cert-title">
          {cert.problem === 'changed'
            ? `The certificate of ${host} changed`
            : `Chord cannot verify the certificate of ${host}`}
        </strong>
        {#if cert.problem === 'changed'}
          <p>
            You pinned another certificate for this account. A server that renews its certificate
            looks like this. So does someone who tries to read your messages. Ask the people who
            run the server if you are not sure.
          </p>
          <span class="meta">Pinned</span>
          <code class="print">{cert.pinned}</code>
          <span class="meta">Now</span>
          <code class="print">{cert.observed}</code>
        {:else}
          <p>
            The server signed its own certificate, or the system does not know who signed it.
            Someone who tries to read your messages would look the same. Trust it only if you
            know this fingerprint, for example from the server owner.
          </p>
          <span class="meta">SHA-256 fingerprint</span>
          <code class="print">{cert.observed ?? 'unknown'}</code>
        {/if}
        <div class="cert-actions">
          <button type="button" class="btn btn-ghost" onclick={() => session.declineCertificate()}>
            Cancel
          </button>
          {#if cert.observed}
            <button
              type="button"
              class="btn btn-danger"
              disabled={busy}
              onclick={() => void session.trustCertificate({ address, password, remember, server })}
            >
              {cert.problem === 'changed' ? 'Trust the new certificate' : 'Trust this certificate'}
            </button>
          {/if}
        </div>
      </div>
    {/if}

    <button class="btn btn-primary btn-lg" type="submit" disabled={busy}>
      {busy ? 'Connecting…' : 'Sign in'}
    </button>
    {#if busy}
      <p class="status" role="status">Connecting to {address.split('@')[1] || 'your server'}…</p>
    {/if}
    <button type="button" class="adv" disabled={busy} onclick={() => (registering = true)}>
      Create an account
    </button>
  </form>
  {/if}
</main>

<style>
  .cert {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-4);
    background: var(--surface-200);
    border: 1px solid var(--danger);
    border-radius: var(--radius-lg);
  }
  .cert p {
    margin: 0;
  }
  .print {
    font-family: var(--font-mono, monospace);
    font-size: 13px;
    overflow-wrap: anywhere;
    user-select: all;
  }
  .cert-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .login {
    height: 100%;
    display: grid;
    place-items: center;
    padding: var(--space-4);
    background: var(--surface-100);
    overflow-y: auto;
  }
  form,
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    width: 360px;
    max-width: 100%;
    padding: var(--space-8);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    animation: arrive var(--dur-slow) var(--ease-out);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .word {
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 32px;
    letter-spacing: -0.03em;
  }
  h1 {
    margin: 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .check input {
    width: 16px;
    height: 16px;
    accent-color: var(--brand);
  }
  .adv {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    align-self: flex-start;
    color: var(--ink-muted);
    font-size: 14px;
  }
  .adv:hover {
    color: var(--ink);
  }
  .adv span {
    display: grid;
    transform: rotate(-90deg);
    transition: transform var(--dur-fast);
  }
  .adv span.open {
    transform: none;
  }
  .error {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    margin: 0;
    color: var(--danger);
  }
  .error :global(svg) {
    flex: none;
    margin-top: 3px;
  }
  .status {
    margin: 0;
    color: var(--ink-muted);
    font-size: 13px;
    text-align: center;
  }
</style>
