<script lang="ts">
  // Change your nickname in the circle you are in.
  import { onMount } from 'svelte';
  import Modal from './Modal.svelte';
  import { app } from './app.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let value = $state('');
  let input = $state<HTMLInputElement>();

  onMount(() => {
    value = app.nickname[app.selectedSpace] ?? app.me.name;
    setTimeout(() => input?.select(), 0);
  });

  function save() {
    const v = value.trim();
    if (!v) return;
    // TODO: await api.changeNick(room, v) for each room of the circle
    app.nickname[app.selectedSpace] = v;
    onclose();
  }
</script>

<Modal title="Change nickname" {onclose}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}
  >
    <div class="field">
      <label for="nick">Nickname in {app.currentSpace?.name ?? 'this circle'}</label>
      <input id="nick" class="input" maxlength="40" bind:this={input} bind:value />
      <span class="meta">Only people in this circle see it.</span>
    </div>
  </form>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={!value.trim()} onclick={save}>Save</button>
  {/snippet}
</Modal>
