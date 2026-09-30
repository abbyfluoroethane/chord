// Syntax highlighting for code blocks. highlight.js (BSD-3) loads on the first block that
// names a language, in its own chunk. The result is a tree of scopes and text. The renderer
// draws the tree with Svelte markup, so no HTML string is ever built or injected.

export interface HNode {
  scope?: string;
  children: (string | HNode)[];
}

type Hljs = typeof import('highlight.js/lib/core').default;

let loaded: Promise<Hljs> | null = null;

// Each name that a user may type maps to the language file that highlight.js has for it.
const LANGUAGES: Record<string, () => Promise<{ default: unknown }>> = {
  javascript: () => import('highlight.js/lib/languages/javascript'),
  typescript: () => import('highlight.js/lib/languages/typescript'),
  json: () => import('highlight.js/lib/languages/json'),
  rust: () => import('highlight.js/lib/languages/rust'),
  python: () => import('highlight.js/lib/languages/python'),
  bash: () => import('highlight.js/lib/languages/bash'),
  css: () => import('highlight.js/lib/languages/css'),
  xml: () => import('highlight.js/lib/languages/xml'),
  diff: () => import('highlight.js/lib/languages/diff'),
  sql: () => import('highlight.js/lib/languages/sql'),
  go: () => import('highlight.js/lib/languages/go'),
  java: () => import('highlight.js/lib/languages/java'),
  c: () => import('highlight.js/lib/languages/c'),
  cpp: () => import('highlight.js/lib/languages/cpp'),
  yaml: () => import('highlight.js/lib/languages/yaml'),
  markdown: () => import('highlight.js/lib/languages/markdown'),
  kotlin: () => import('highlight.js/lib/languages/kotlin'),
  swift: () => import('highlight.js/lib/languages/swift')
};

const ALIASES: Record<string, string> = {
  js: 'javascript',
  jsx: 'javascript',
  mjs: 'javascript',
  ts: 'typescript',
  tsx: 'typescript',
  rs: 'rust',
  py: 'python',
  sh: 'bash',
  shell: 'bash',
  zsh: 'bash',
  console: 'bash',
  html: 'xml',
  svg: 'xml',
  golang: 'go',
  'c++': 'cpp',
  h: 'c',
  hpp: 'cpp',
  yml: 'yaml',
  md: 'markdown',
  kt: 'kotlin',
  patch: 'diff',
  jsonc: 'json'
};

/** The highlight.js name for a language that a user typed, or null. */
export function languageName(lang: string): string | null {
  const key = lang.toLowerCase();
  const name = ALIASES[key] ?? key;
  return name in LANGUAGES ? name : null;
}

async function engine(): Promise<Hljs> {
  loaded ??= import('highlight.js/lib/core').then((m) => m.default);
  return loaded;
}

const registered = new Set<string>();

/** The highlighted tree, or null when the language is unknown or the code is too long. */
export async function highlightCode(code: string, lang: string): Promise<HNode | null> {
  const name = languageName(lang);
  if (!name || code.length > 20000) return null;
  const hljs = await engine();
  if (!registered.has(name)) {
    hljs.registerLanguage(name, (await LANGUAGES[name]()).default as never);
    registered.add(name);
  }
  const result = hljs.highlight(code, { language: name, ignoreIllegals: true });
  return (result._emitter as unknown as { root: HNode }).root;
}
