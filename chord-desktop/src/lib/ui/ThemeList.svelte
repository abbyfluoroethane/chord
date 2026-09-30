<script lang="ts">
  // The themes for one mode, as cards with a small preview, and the accents of the
  // picked theme as round swatches.
  import Check from 'lucide-svelte/icons/check';
  import LinkIcon from 'lucide-svelte/icons/link';
  import Trash2 from 'lucide-svelte/icons/trash-2';
  import { theme } from '$lib/theme/theme.svelte';
  import { hostOf, lineChange } from '$lib/theme/themelink';
  import { sanitizeThemeCss } from '$lib/theme/themesafe';
  import type { ThemeMode } from '$lib/theme/themecss';
  import Icon from './Icon.svelte';

  let { mode }: { mode: ThemeMode } = $props();

  const themes = $derived(theme.library.filter((t) => t.info.mode === mode));
  const picked = $derived(theme.themeFor(mode));
  const accent = $derived(theme.accentOf(picked));
</script>

<div class="list" role="radiogroup" aria-label="{mode === 'dark' ? 'Dark' : 'Light'} theme">
  {#each themes as t (t.id)}
    {@const on = t.id === picked.id}
    <div class="card" class:on class:removable={!t.builtIn}>
      <button class="pick" role="radio" aria-checked={on} onclick={() => theme.use(t.id)}>
        <span
          class="swatch"
          aria-hidden="true"
          style:background={t.info.swatch.surface}
          style:--panel={t.info.swatch.panel}
          style:--ink={t.info.swatch.ink}
          style:--dot={t.id === picked.id && accent
            ? (t.info.accents.find((a) => a.id === accent)?.color ?? t.info.swatch.brand)
            : t.info.swatch.brand}
        >
          <i class="panel"></i><i class="line"></i><i class="line short"></i><i class="dot"></i>
        </span>
        <span class="text">
          <span class="name">{t.info.name}</span>
          <span class="meta">
            {#if t.url}
              <span class="linked" title={t.url}>
                <Icon icon={LinkIcon} size={12} />{hostOf(t.url)}
              </span>
            {:else}
              {t.builtIn ? 'Built in' : (t.info.author ?? 'Imported')}
            {/if}
          </span>
        </span>
        {#if on}<span class="check"><Icon icon={Check} size={16} /></span>{/if}
      </button>
      {#if t.pending !== undefined}
        {@const change = lineChange(t.css, t.pending)}
        {@const blocked = sanitizeThemeCss(t.pending).blocked.length}
        <div class="update" role="status">
          <span>
            New version from {t.url ? hostOf(t.url) : 'the link'}: {change.added} lines added,
            {change.removed} removed.
            {#if blocked}It asks for {blocked} {blocked === 1 ? 'request' : 'requests'} to other
              hosts. Chord blocks {blocked === 1 ? 'it' : 'them'}.{/if}
          </span>
          <span class="actions">
            <button class="btn" onclick={() => theme.acceptUpdate(t.id)}>Update</button>
            <button class="btn btn-ghost" onclick={() => theme.dismissUpdate(t.id)}>Skip</button>
          </span>
        </div>
      {/if}
      {#if !t.builtIn}
        <button class="remove" aria-label="Remove {t.info.name}" onclick={() => theme.remove(t.id)}>
          <Icon icon={Trash2} size={16} />
        </button>
      {/if}
    </div>
  {/each}
</div>

{#if picked.info.accents.length > 1}
  <div class="accents" role="radiogroup" aria-label="Accent for {picked.info.name}">
    {#each picked.info.accents as a (a.id)}
      <button
        class="accent"
        class:on={a.id === accent}
        role="radio"
        aria-checked={a.id === accent}
        aria-label={a.name}
        title={a.name}
        style:background={a.color ?? 'var(--brand)'}
        onclick={() => theme.setAccent(picked.id, a.id)}
      ></button>
    {/each}
  </div>
{/if}

<style>
  .list {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .card {
    position: relative;
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      background var(--dur-fast) var(--ease-out);
  }
  .card:hover {
    border-color: var(--ink-muted);
  }
  .card.on {
    background: var(--selected);
    border-color: var(--ink-muted);
  }
  .pick {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    padding: var(--space-2);
    text-align: left;
    border-radius: var(--radius-md);
  }
  /* A tiny window in the theme's own colours: a panel, two text lines, an accent dot. */
  .swatch {
    position: relative;
    flex: none;
    width: 64px;
    height: 44px;
    overflow: hidden;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
  .swatch i {
    position: absolute;
    display: block;
  }
  .swatch .panel {
    inset: 0 auto 0 0;
    width: 18px;
    background: var(--panel);
  }
  .swatch .line {
    left: 24px;
    top: 10px;
    width: 30px;
    height: 4px;
    border-radius: 2px;
    background: var(--ink);
    opacity: 0.8;
  }
  .swatch .short {
    top: 20px;
    width: 20px;
    opacity: 0.5;
  }
  .swatch .dot {
    left: 24px;
    bottom: 7px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--dot);
  }
  .text {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    overflow: hidden;
    color: var(--ink-muted);
    font-size: 12px;
    font-weight: 500;
    line-height: 16px;
    letter-spacing: 0.02em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .linked {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 100%;
  }
  .check {
    display: grid;
    color: var(--brand);
  }
  .removable .pick {
    padding-right: 44px;
  }
  .remove {
    position: absolute;
    top: 50%;
    right: var(--space-2);
    transform: translateY(-50%);
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-md);
    color: var(--ink-muted);
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }
  .remove:hover {
    background: var(--hover);
    color: var(--danger);
  }
  .update {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2);
    border-top: 1px solid var(--line);
    font-size: 12px;
    line-height: 16px;
    color: var(--ink-muted);
  }
  .actions {
    display: flex;
    gap: var(--space-2);
  }
  .accents {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }
  .accent {
    width: 24px;
    height: 24px;
    border: 1px solid var(--line);
    border-radius: 50%;
    transition: outline-color var(--dur-fast) var(--ease-out);
    outline: 2px solid transparent;
    outline-offset: 2px;
  }
  .accent:hover {
    outline-color: var(--ink-muted);
  }
  .accent.on {
    outline-color: var(--ink);
  }
</style>
