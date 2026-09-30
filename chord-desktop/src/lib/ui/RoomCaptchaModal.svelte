<script lang="ts">
  // A room holds our join until we solve a CAPTCHA (XEP-0158). The form is a data form, so
  // the generic renderer shows it, image included. A wrong answer keeps the dialog open
  // with the error of the room. The room may send a new challenge: the dialog then
  // shows the new form.
  import { untrack } from 'svelte';
  import type { DataForm } from '$lib/chord/types';
  import DataFormView from './DataFormView.svelte';
  import Modal from './Modal.svelte';
  import { plainError } from './adapt';
  import { api } from './bridge';
  import { captchaProblems } from './roomjoin';
  import type { CaptchaAsk } from './ui.svelte';

  let { ask, onclose }: { ask: CaptchaAsk; onclose: () => void } = $props();

  // The answers live in a copy, so that a new challenge replaces the form cleanly.
  let form = $state<DataForm>(untrack(() => structuredClone($state.snapshot(ask.form))));
  let busy = $state(false);
  let error = $state('');
  let showProblems = $state(false);

  $effect(() => {
    form = structuredClone($state.snapshot(ask.form));
    error = '';
    showProblems = false;
  });

  async function send() {
    if (captchaProblems(form).length) {
      showProblems = true;
      return;
    }
    busy = true;
    error = '';
    try {
      await (await api()).answerRoomCaptcha(ask.room, $state.snapshot(form));
      onclose();
    } catch (e) {
      error = plainError(e);
    } finally {
      busy = false;
    }
  }

  async function cancel() {
    onclose();
    try {
      await (await api()).cancelRoomCaptcha(ask.room);
    } catch {
      /* The join ends either way. */
    }
  }
</script>

<Modal title="Prove that you are a person" onclose={cancel}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void send();
    }}
  >
    <DataFormView bind:form disabled={busy} {showProblems} idPrefix="room-captcha" />
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </form>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={cancel}>Cancel</button>
    <button class="btn btn-primary" disabled={busy} onclick={send}>Send</button>
  {/snippet}
</Modal>

<style>
  .error {
    margin: var(--space-2) 0 0;
    color: var(--danger);
  }
</style>
