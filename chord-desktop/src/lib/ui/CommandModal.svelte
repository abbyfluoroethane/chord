<script lang="ts">
  // Runs one ad-hoc command (XEP-0050), step by step. Each step shows the form that the
  // service sends, with the buttons that it allows. The flow rules are in forms.ts.
  import { onMount } from 'svelte';
  import type { CommandAction, CommandItem, CommandStep, DataForm } from '$lib/chord/types';
  import { sampleSteps } from '$lib/fixtures/forms';
  import DataFormView from './DataFormView.svelte';
  import Modal from './Modal.svelte';
  import { plainError } from './adapt';
  import { api, live } from './bridge';
  import { commandTitle, isFinished, stepButtons, stepRequest } from './forms';

  let { item, onclose }: { item: CommandItem; onclose: () => void } = $props();

  let step = $state<CommandStep | null>(null);
  /** The form of the step. The user changes it, and the next call sends it. */
  let form = $state<DataForm | null>(null);
  let busy = $state(true);
  let error = $state('');
  let showProblems = $state(false);
  let previewAt = 0;

  onMount(() => {
    void run('execute');
    // Leaving in the middle of a command frees it on the server.
    return () => {
      if (live && step && !isFinished(step) && step.sessionId) {
        void api().then((b) =>
          b.commandStep(item.jid, item.node, step?.sessionId ?? null, 'cancel', null).catch(() => {})
        );
      }
    };
  });

  async function call(action: CommandAction, sessionId: string | null, answer: DataForm | null) {
    if (live) return (await api()).commandStep(item.jid, item.node, sessionId, action, answer);
    // The preview has no bridge. It walks through two sample steps.
    const steps = sampleSteps();
    previewAt = action === 'cancel' ? steps.length - 1 : Math.min(previewAt + 1, steps.length - 1);
    return steps[action === 'execute' ? 0 : previewAt];
  }

  async function run(action: CommandAction) {
    error = '';
    const r = step && action !== 'execute' ? stepRequest(step, action, form) : null;
    if (r && 'problems' in r) {
      showProblems = true;
      error = r.problems[0];
      return;
    }
    busy = true;
    try {
      const sent = r && 'request' in r ? r.request : { sessionId: null, action, form: null };
      step = await call(sent.action, sent.sessionId, sent.form);
      form = step.form;
      showProblems = false;
    } catch (e) {
      error = plainError(e);
    } finally {
      busy = false;
    }
  }

  const buttons = $derived(step ? stepButtons(step.actions, step.defaultAction) : []);
  const finished = $derived(step ? isFinished(step) : false);
</script>

<Modal title={commandTitle(item)} size="medium" {onclose}>
  {#if step}
    {#each step.notes as note, i (i)}
      <p class="note" class:warn={note.kind === 'warn'} class:bad={note.kind === 'error'} role="status">
        {note.text}
      </p>
    {/each}
    {#if form}
      <DataFormView
        bind:form
        idPrefix="cmd"
        disabled={busy || finished}
        {showProblems}
      />
    {:else if step.notes.length === 0}
      <p class="note">{finished ? 'The command is done.' : 'The service sent no form.'}</p>
    {/if}
  {:else if busy}
    <p class="note" role="status">Starting…</p>
  {/if}
  {#if error}<p class="err" role="alert">{error}</p>{/if}

  {#snippet footer()}
    {#if finished}
      <button class="btn btn-primary" onclick={onclose}>Done</button>
    {:else}
      <button class="btn btn-ghost" disabled={busy} onclick={() => void run('cancel')}>Cancel</button>
      {#each buttons as b (b.action)}
        <button
          class="btn"
          class:btn-primary={b.primary}
          disabled={busy || !step}
          onclick={() => void run(b.action)}
        >
          {b.action === 'prev' ? 'Back' : b.action === 'next' ? 'Next' : 'Finish'}
        </button>
      {/each}
    {/if}
  {/snippet}
</Modal>

<style>
  .note {
    margin: 0 0 var(--space-3);
    white-space: pre-line;
  }
  .warn {
    color: var(--warning, var(--ink));
  }
  .bad,
  .err {
    color: var(--danger);
  }
  .err {
    margin: var(--space-3) 0 0;
  }
</style>
