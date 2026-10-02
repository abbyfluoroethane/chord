<script lang="ts">
  // Members grouped by affiliation and presence: owners, admins, online, visitors, offline.
  import MemberRow from './MemberRow.svelte';
  import { app } from './app.svelte';
  import { groupMembers } from './rooms';

  const groups = $derived(groupMembers(app.membersHere));
</script>

<aside class="members" aria-label="Members">
  {#each groups as g (g.label)}
    <section aria-label={g.label}>
      <h2 class="group">{g.label} — {g.items.length}</h2>
      {#each g.items as m (m.id)}
        <MemberRow member={m} />
      {/each}
    </section>
  {/each}
</aside>

<style>
  .members {
    width: var(--members-width);
    flex: none;
    overflow-y: auto;
    padding-bottom: var(--space-4);
    background: var(--surface-side);
    border-left: 1px solid var(--line-strong);
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  .group {
    margin: 0;
    height: 40px;
    padding: 24px var(--space-2) 0 var(--space-4);
    font-size: 12px;
    line-height: 16px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: color-mix(in srgb, var(--ink) 88%, var(--surface-side));
  }
</style>
