<script lang="ts">
  // The "Add contact" view: one address field with an inline "Send request" button.
  import { contactsStore } from './contacts.svelte';

  let address = $state('');
  let result = $state<{ ok: boolean; text: string } | null>(null);

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!address.trim()) return;
    const r = contactsStore.add(address);
    if (r.ok) {
      result = { ok: true, text: r.message };
      address = '';
    } else {
      result = { ok: false, text: r.error };
    }
  }
</script>

<div class="add">
  <h2 class="title">Add contact</h2>
  <p class="hint">Enter their address, like sam@chord.example.</p>

  <form onsubmit={submit}>
    <div class="box" class:bad={result && !result.ok}>
      <input
        type="text"
        inputmode="email"
        autocomplete="off"
        spellcheck="false"
        placeholder="name@chord.example"
        aria-label="Address"
        aria-invalid={result && !result.ok ? 'true' : undefined}
        aria-describedby="add-result"
        bind:value={address}
        oninput={() => (result = null)}
      />
      <button class="btn btn-primary" type="submit" disabled={!address.trim()}>Send request</button>
    </div>
    <p id="add-result" class="result" class:ok={result?.ok} class:err={result && !result.ok} role="status">
      {result?.text ?? ''}
    </p>
  </form>
</div>

<style>
  .add {
    padding: var(--space-6);
    max-width: 640px;
  }
  h2 {
    margin: 0;
  }
  .hint {
    margin: var(--space-1) 0 var(--space-4);
    color: var(--ink-muted);
  }
  .box {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-300);
    transition: border-color var(--dur-fast);
  }
  .box:focus-within {
    border-color: var(--accent);
  }
  .box.bad {
    border-color: var(--danger);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 36px;
    padding: 0 var(--space-2);
    background: none;
    border: 0;
    outline: 0;
    font-family: var(--font-mono);
    font-size: 14px;
  }
  input::placeholder {
    font-family: var(--font-sans);
    color: var(--ink-muted);
  }
  .result {
    min-height: 22px;
    margin: var(--space-2) 0 0;
    font-size: 14px;
  }
  .result.ok {
    color: var(--online);
  }
  .result.err {
    color: var(--danger);
  }
</style>
