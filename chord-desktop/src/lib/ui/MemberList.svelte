<script lang="ts">
  // Members grouped by affiliation and presence: owners, admins, online, offline.
  import MemberRow from './MemberRow.svelte';
  import { app } from './app.svelte';
  import type { MemberItem } from './types';

  const groups = $derived.by(() => {
    const m = app.membersHere;
    const on = m.filter((x) => x.online);
    const list: { label: string; items: MemberItem[] }[] = [
      { label: 'Owners', items: on.filter((x) => x.affiliation === 'owner') },
      { label: 'Admins', items: on.filter((x) => x.affiliation === 'admin') },
      { label: 'Online', items: on.filter((x) => x.affiliation !== 'owner' && x.affiliation !== 'admin') },
      { label: 'Offline', items: m.filter((x) => !x.online) }
    ];
    return list.filter((g) => g.items.length);
  });
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
    background: var(--surface-200);
    border-left: 1px solid var(--line);
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  .group {
    margin: 0;
    height: 40px;
    padding: 24px var(--space-2) 0 var(--space-4);
    font-size: 12px;
    line-height: 16px;
    font-weight: 500;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }
</style>
