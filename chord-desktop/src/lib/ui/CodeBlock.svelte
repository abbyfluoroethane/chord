<script lang="ts">
  // A fenced code block. With a known language, the code shows in colour after the
  // highlighter loads. Before that, and for other languages, it shows as plain text.
  import { highlightCode, type HNode } from './highlight';

  let { code, lang }: { code: string; lang: string } = $props();

  let tree = $state<HNode | null>(null);

  $effect(() => {
    const c = code;
    const l = lang;
    tree = null;
    if (!l) return;
    let live = true;
    highlightCode(c, l)
      .then((t) => {
        if (live) tree = t;
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  });

  /** "title.function_" becomes "title": the class for the colour. */
  function cls(scope?: string): string {
    return scope && !scope.startsWith('language:') ? `hl-${scope.split('.')[0]}` : '';
  }
</script>

{#snippet nodes(list: (string | HNode)[])}{#each list as n, i (i)}{#if typeof n === 'string'}{n}{:else}<span
        class={cls(n.scope)}>{@render nodes(n.children)}</span
      >{/if}{/each}{/snippet}

<pre class="block"><code
    >{#if tree}{@render nodes(tree.children)}{:else}{code}{/if}</code
  ></pre>

<style>
  .block {
    box-sizing: border-box;
    max-width: 100%;
    margin: 4px 0;
    padding: 8px 12px;
    overflow-x: auto;
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    font-family: var(--font-mono);
    font-size: 13px;
    line-height: 20px;
    white-space: pre;
    overflow-wrap: normal;
  }
  code {
    font: inherit;
  }
  /*
   * Colours come from the Chord tokens only. Amber marks keywords and types. Green marks
   * strings. Teal marks numbers and names. Muted grey marks comments. The amber text token
   * is the darker one in light mode, so it stays readable.
   */
  .hl-keyword,
  .hl-built_in,
  .hl-type,
  .hl-literal,
  .hl-selector-tag,
  .hl-meta {
    color: var(--brand-ink);
  }
  .hl-string,
  .hl-regexp,
  .hl-symbol,
  .hl-template-variable,
  .hl-addition {
    color: var(--online);
  }
  .hl-number,
  .hl-title,
  .hl-section,
  .hl-tag,
  .hl-name,
  .hl-attr,
  .hl-attribute,
  .hl-bullet {
    color: var(--accent);
  }
  .hl-comment,
  .hl-quote,
  .hl-doctag {
    color: var(--ink-muted);
    font-style: italic;
  }
  .hl-deletion {
    color: var(--danger);
  }
  .hl-emphasis {
    font-style: italic;
  }
  .hl-strong {
    font-weight: 600;
  }
</style>
