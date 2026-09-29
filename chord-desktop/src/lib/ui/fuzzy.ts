// Small fuzzy matcher for the quick switcher. Subsequence match with bonuses.

export function score(query: string, text: string): number {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (!q) return 1;
  let ti = 0;
  let s = 0;
  let streak = 0;
  for (const ch of q) {
    const at = t.indexOf(ch, ti);
    if (at < 0) return 0;
    streak = at === ti ? streak + 1 : 0;
    s += 1 + streak * 2 + (at === 0 || /[\s\-_/#@.]/.test(t[at - 1]) ? 4 : 0);
    ti = at + 1;
  }
  if (t.startsWith(q)) s += 10;
  return Math.max(s - t.length * 0.05, 0.01);
}
