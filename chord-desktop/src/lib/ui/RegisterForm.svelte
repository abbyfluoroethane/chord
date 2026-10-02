<script lang="ts">
  // Create an account on a server (XEP-0077 in-band registration), before any login.
  // Step 1 asks for the server and reads its registration form. Step 2 shows the fields
  // (the legacy ones, or a data form with a CAPTCHA) and sends the answers. A server that
  // only gives a link shows the link. Many servers keep registration closed.
  import { untrack } from 'svelte';
  import ArrowLeft from 'lucide-svelte/icons/arrow-left';
  import CircleAlert from 'lucide-svelte/icons/circle-alert';
  import type { DataForm, RegistrationForm } from '$lib/chord/types';
  import { sampleRegistration } from '$lib/fixtures/forms';
  import DataFormView from './DataFormView.svelte';
  import Icon from './Icon.svelte';
  import { api, live } from './bridge';
  import { openLink } from './attachments';
  import {
    credentialsOf,
    domainOf,
    legacyAnswer,
    legacyFields,
    legacyMissing,
    problems,
    registerErrorText,
    submission
  } from './forms';
  import { serverArg } from './session.svelte';

  let {
    address = '',
    server = '',
    onback,
    onregistered
  }: {
    /** What the person typed in the sign-in form. Its server part is the first guess. */
    address?: string;
    /** The "Server" field of the sign-in form (host:port), or empty. */
    server?: string;
    onback: () => void;
    /** The account exists. The sign-in form takes over with these values. */
    onregistered: (address: string, password: string) => void;
  } = $props();

  // The first guess comes from the sign-in form. After that, the person owns the value.
  let domain = $state(untrack(() => domainOf(address)));
  let asked = $state<RegistrationForm | null>(null);
  let form = $state<DataForm | null>(null);
  let values = $state<Record<string, string>>({});
  let busy = $state(false);
  let error = $state('');
  let showProblems = $state(false);

  const fields = $derived(asked ? legacyFields(asked.fields) : []);

  async function fetchForm(e: SubmitEvent) {
    e.preventDefault();
    const d = domainOf(domain);
    if (!d) return;
    busy = true;
    error = '';
    try {
      const got = live
        ? await (await api()).registrationForm(d, serverArg(server) || undefined)
        : sampleRegistration;
      asked = got;
      form = got.form;
      values = {};
      domain = d;
    } catch (err) {
      error = registerErrorText(err);
    } finally {
      busy = false;
    }
  }

  async function send(e: SubmitEvent) {
    e.preventDefault();
    if (!asked) return;
    error = '';
    let answer;
    let username = values.username?.trim() ?? '';
    let password = values.password ?? '';
    if (form) {
      const found = problems(form);
      if (found.length > 0) {
        showProblems = true;
        error = found[0];
        return;
      }
      ({ username, password } = credentialsOf(form));
      answer = { form: submission(form) };
    } else {
      const missing = legacyMissing(asked.fields, values);
      if (missing.length > 0) {
        error = `Fill in: ${missing.join(', ')}.`;
        return;
      }
      answer = { fields: legacyAnswer(asked.fields, values) };
    }
    busy = true;
    try {
      if (live) await (await api()).registerAccount(domain, answer, serverArg(server) || undefined);
      onregistered(username ? `${username}@${domain}` : '', password);
    } catch (err) {
      error = registerErrorText(err);
    } finally {
      busy = false;
    }
  }

  /** Each step puts the focus on its first field. */
  function focusFirst(form: HTMLFormElement) {
    form
      .querySelector<HTMLElement>('input:not([type=hidden]):not([disabled]), select, textarea')
      ?.focus();
  }
  const linkOnly = $derived(!!asked && !asked.form && asked.fields.length === 0);
</script>

<div class="register">
  <button type="button" class="back" onclick={asked ? () => (asked = null) : onback}>
    <Icon icon={ArrowLeft} size={16} /> Back
  </button>
  <h1 class="title">Create an account</h1>

  {#if !asked}
    <form use:focusFirst onsubmit={fetchForm} novalidate>
      <div class="field">
        <label for="reg-domain">Server</label>
        <input
          id="reg-domain"
          class="input mono"
          placeholder="example.com"
          autocapitalize="off"
          spellcheck="false"
          bind:value={domain}
          disabled={busy}
        />
        <span class="meta">The address of the server that hosts your account.</span>
      </div>
      {#if error}
        <p class="error" role="alert"><Icon icon={CircleAlert} size={16} /> {error}</p>
      {/if}
      <button class="btn btn-primary btn-lg" type="submit" disabled={busy || !domainOf(domain)}>
        {busy ? 'Connecting…' : 'Continue'}
      </button>
    </form>
  {:else}
    <form use:focusFirst onsubmit={send} novalidate>
      {#if asked.instructions}<p class="instructions">{asked.instructions}</p>{/if}

      {#if form}
        <DataFormView bind:form idPrefix="reg" disabled={busy} {showProblems} />
      {:else}
        {#each fields as f (f.name)}
          <div class="field">
            <label for="reg-{f.name}">{f.label}</label>
            <input
              id="reg-{f.name}"
              class="input"
              class:mono={f.name === 'username'}
              type={f.secret ? 'password' : 'text'}
              autocomplete={f.secret ? 'new-password' : f.name === 'username' ? 'username' : 'off'}
              autocapitalize="off"
              spellcheck="false"
              bind:value={values[f.name]}
              disabled={busy}
            />
          </div>
        {/each}
      {/if}

      {#if asked.oob}
        <p class="instructions">
          {asked.oob.desc ?? 'The server also has a web page for this.'}
          <button type="button" class="link" onclick={() => void openLink(asked!.oob!.url)}>
            Open the web page
          </button>
        </p>
      {/if}

      {#if error}
        <p class="error" role="alert"><Icon icon={CircleAlert} size={16} /> {error}</p>
      {/if}
      {#if linkOnly}
        <p class="instructions">This server does not take new accounts in the app.</p>
      {:else}
        <button class="btn btn-primary btn-lg" type="submit" disabled={busy}>
          {busy ? 'Creating…' : 'Create account'}
        </button>
      {/if}
    </form>
  {/if}
</div>

<style>
  .register,
  form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  h1 {
    margin: 0;
  }
  .back {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    align-self: flex-start;
    color: var(--ink-muted);
    font-size: 14px;
  }
  .back:hover {
    color: var(--ink);
  }
  .instructions {
    margin: 0;
    white-space: pre-line;
    color: var(--ink-muted);
  }
  .link {
    color: var(--accent);
    text-decoration: underline;
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
</style>
