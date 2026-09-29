<script lang="ts">
  // Hosts the person views: card, menu, full profile, nickname. Any row can open them through ui.
  import NicknameModal from './NicknameModal.svelte';
  import PersonMenu from './PersonMenu.svelte';
  import ProfileModal from './ProfileModal.svelte';
  import ProfilePopout from './ProfilePopout.svelte';
  import { ui } from './ui.svelte';
</script>

{#if ui.popout}
  {#key ui.popout}<ProfilePopout state={ui.popout} />{/key}
{/if}
{#if ui.personMenu}
  {#key ui.personMenu}<PersonMenu state={ui.personMenu} />{/key}
{/if}
{#if ui.profile}
  {#key ui.profile}<ProfileModal address={ui.profile} onclose={() => (ui.profile = null)} />{/key}
{/if}
{#if ui.nicknameOpen}<NicknameModal onclose={() => (ui.nicknameOpen = false)} />{/if}
