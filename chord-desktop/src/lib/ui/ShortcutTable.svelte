<script lang="ts">
  import { shortcuts } from './shortcuts';
</script>

<ul>
  {#each shortcuts as r (r.what)}
    <li>
      <span>{r.what}</span>
      <span class="keys">
        {#each r.keys as k, i (i)}<kbd>{k}</kbd>{/each}
      </span>
    </li>
  {/each}
</ul>

<style>
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
  .keys {
    display: flex;
    gap: var(--space-1);
    flex: none;
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
