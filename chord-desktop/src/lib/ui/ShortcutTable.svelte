<script lang="ts">
  import { shortcutGroups, shortcuts } from './shortcuts';
</script>

{#each shortcutGroups as group (group)}
  <h3>{group}</h3>
  <ul>
    {#each shortcuts.filter((s) => s.group === group) as r (r.what)}
      <li>
        <span>{r.what}</span>
        <span class="sets">
          {#each [r.keys, ...(r.alternates ?? [])] as set, n (n)}
            {#if n > 0}<span class="or">or</span>{/if}
            <span class="keys">
              {#each set as k, i (i)}<kbd>{k}</kbd>{/each}
            </span>
          {/each}
        </span>
      </li>
    {/each}
  </ul>
{/each}

<style>
  h3 {
    margin: var(--space-4) 0 var(--space-1);
    color: var(--ink-muted);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  h3:first-child {
    margin-top: 0;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--line);
  }
  li:last-child {
    border-bottom: 0;
  }
  .sets {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    flex: none;
    max-width: 60%;
  }
  .or {
    color: var(--ink-muted);
    font-size: 12px;
  }
  .keys {
    display: flex;
    gap: var(--space-1);
  }
  kbd {
    min-width: 24px;
    padding: 2px 6px;
    text-align: center;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 16px;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
</style>
