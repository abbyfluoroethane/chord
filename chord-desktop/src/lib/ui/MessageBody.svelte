<script lang="ts">
  // Message text with Discord markdown. The parser makes a tree and this file draws it with
  // Svelte markup. No {@html}. Links are http, https, or xmpp:. A click on an xmpp: link
  // opens the question dialog of Chord and never goes to the OS.
  import { app } from './app.svelte';
  import { prefs } from './prefs.svelte';
  import CodeBlock from './CodeBlock.svelte';
  import EmojiText from './EmojiText.svelte';
  import { parseMarkdown, type Block, type Inline } from './markdown';
  import Spoiler from './Spoiler.svelte';
  import { loadShortcodes, mayHaveShortcode, shortcodesNow, type Shortcodes } from './shortcodes';
  import TimeChip from './TimeChip.svelte';
  import { xmppLinks } from './xmpplinks.svelte';
  import { leaving } from './leaving.svelte';
  import { maskedMismatch, maskedTitle } from './linkguard';

  let { body }: { body: string } = $props();

  // The shortcode data loads only when a message may hold a shortcode. Until then, and for
  // an unknown name, the text shows as it is.
  let codes = $state<Shortcodes | null>(shortcodesNow());
  $effect(() => {
    if (codes || !mayHaveShortcode(body)) return;
    loadShortcodes()
      .then((m) => (codes = m))
      .catch(() => {});
  });

  /** The plain text of link children, as the message shows it. */
  function textOf(nodes: Inline[]): string {
    return nodes
      .map((n) => ('v' in n && typeof n.v === 'string' ? n.v : 'children' in n ? textOf(n.children) : ''))
      .join('');
  }

  // A masked link whose text names another host asks "Leave Chord?" first (BRIDGESECURITY-12).
  function leave(e: MouseEvent, href: string, children: Inline[]) {
    const text = textOf(children);
    if (!maskedMismatch(text, href)) return;
    e.preventDefault();
    leaving.ask(href, text);
  }

  // The prop is a getter. This value stays equal while the text stays equal, so the parser
  // runs only when the text changes.
  const source = $derived(body);
  const parsed = $derived(
    parseMarkdown(source, {
      myNames: [app.me.name, app.me.address.split('@')[0], app.me.address],
      shortcodes: codes
    })
  );
</script>

{#snippet inline(nodes: Inline[])}{#each nodes as n, i (i)}{#if n.t === 'text'}<EmojiText
        text={n.v}
      />{:else if n.t === 'code'}<code class="inline">{n.v}</code
      >{:else if n.t === 'link' && n.xmpp}<a
        href={n.href}
        title={n.masked ? n.href : undefined}
        onclick={(e) => {
          e.preventDefault();
          xmppLinks.open(n.href);
        }}>{@render inline(n.children)}</a
      >{:else if n.t === 'link'}<a
        href={n.href}
        title={n.masked ? maskedTitle(n.href) : undefined}
        onclick={n.masked ? (e) => leave(e, n.href, n.children) : undefined}
        target="_blank"
        rel="noopener noreferrer">{@render inline(n.children)}</a
      >{:else if n.t === 'mention'}<span class="mention" class:me={n.me}>{n.v}</span
      >{:else if n.t === 'time'}<TimeChip seconds={n.seconds} style={n.style} />{:else if n.t === 'bold'}<strong
        >{@render inline(n.children)}</strong
      >{:else if n.t === 'italic'}<em>{@render inline(n.children)}</em
      >{:else if n.t === 'underline'}<u>{@render inline(n.children)}</u
      >{:else if n.t === 'strike'}<s>{@render inline(n.children)}</s
      >{:else if n.t === 'spoiler'}<Spoiler>{@render inline(n.children)}</Spoiler>{/if}{/each}{/snippet}

{#snippet blocks(list: Block[])}{#each list as b, i (i)}{#if b.t === 'para'}<div class="para">{@render inline(b.children)}</div
      >{:else if b.t === 'code'}<CodeBlock code={b.v} lang={b.lang} />{:else if b.t === 'quote'}<blockquote
        class="quote">{@render blocks(b.children)}</blockquote
      >{:else if b.t === 'heading'}<div class="heading h{b.level}" role="heading" aria-level={b.level}
        >{@render inline(b.children)}</div
      >{:else if b.t === 'subtext'}<div class="subtext">{@render inline(b.children)}</div
      >{:else if b.t === 'list'}{#if b.ordered}<ol start={b.start}>{@render items(b)}</ol
        >{:else}<ul>{@render items(b)}</ul>{/if}{/if}{/each}{/snippet}

{#snippet items(list: Extract<Block, { t: 'list' }>)}{#each list.items as item, i (i)}<li
      >{@render inline(item.content)}{#if item.sub}{@render blocks([item.sub])}{/if}</li
    >{/each}{/snippet}

<div class="body" class:jumbo={parsed.jumbo && prefs.jumboEmoji}>{@render blocks(parsed.blocks)}</div>

<style>
  .body {
    overflow-wrap: anywhere;
    min-width: 0;
  }
  /* Only text blocks keep the line breaks of the message. */
  .para,
  .heading,
  .subtext {
    white-space: pre-wrap;
  }
  :global(:root[data-links='hover']) .body a {
    text-decoration: none;
  }
  :global(:root[data-links='hover']) .body a:hover {
    text-decoration: underline;
  }
  .jumbo {
    font-size: 40px;
    line-height: 48px;
  }
  .heading {
    margin: 8px 0 0;
    font-weight: 600;
  }
  .heading:first-child {
    margin-top: 0;
  }
  .h1 {
    font-size: 24px;
    line-height: 30px;
  }
  .h2 {
    font-size: 20px;
    line-height: 26px;
  }
  .h3 {
    font-size: 16px;
    line-height: 22px;
  }
  .subtext {
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 16px;
    font-weight: 500;
    letter-spacing: 0.02em;
  }
  .inline {
    padding: 1px 4px;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 0.9em;
  }
  .quote {
    position: relative;
    margin: 4px 0;
    padding-left: 16px;
  }
  .quote::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: 4px;
    border-radius: 2px;
    background: var(--line);
  }
  ul,
  ol {
    margin: 4px 0;
    padding-left: 1.5em;
  }
  ul {
    list-style: disc;
  }
  ul ul {
    list-style: circle;
  }
  ul ul ul {
    list-style: square;
  }
  ul ul,
  ol ol,
  ul ol,
  ol ul {
    margin: 0;
  }
  .mention {
    padding: 0 2px;
    border-radius: var(--radius-sm);
    background: var(--brand-soft);
    color: var(--brand-ink);
    font-weight: 500;
  }
</style>
