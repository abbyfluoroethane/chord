// Marketing screenshots of the desktop preview (the sample data in src/lib/fixtures).
// See README.md in this folder for how to run it.
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// The host name is not local on purpose: Chord never loads media from a local address, so
// on 127.0.0.1 the photos would show as file cards. Chromium maps the name to 127.0.0.1.
const PORT = process.env.PORT ?? '4801';
const BASE = `http://preview.chord.example:${PORT}/index.html`;
const OUT =
  process.env.OUT ?? fileURLToPath(new URL('../../../docs/brand/screenshots/desktop/', import.meta.url));
const ONLY = process.argv.slice(2);
mkdirSync(OUT, { recursive: true });

const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM || undefined,
  args: ['--no-sandbox', '--host-resolver-rules=MAP preview.chord.example 127.0.0.1']
});

/**
 * Zoomed in: a 1000x625 window at a scale of 2.88 gives a 2880x1800 PNG, so the app looks
 * like it does at about 150 % zoom. Cozy display and the default font size.
 */
const W = 1000;
const H = 625;
const SCALE = 2.88;
/** Room around a cropped element, in CSS pixels. */
const PAD = 16;

async function open(hash, mode, { members = false, h = H } = {}) {
  const ctx = await browser.newContext({
    viewport: { width: W, height: h },
    deviceScaleFactor: SCALE,
    colorScheme: mode
  });
  await ctx.addInitScript(
    ([mode, members]) => {
      localStorage.setItem('chord.prefs', JSON.stringify({ display: 'cozy' }));
      localStorage.setItem('chord.theme', mode);
      localStorage.setItem('chord.membersOpen', members ? '1' : '0');
    },
    [mode, members]
  );
  const page = await ctx.newPage();
  page.on('pageerror', (e) => console.error('pageerror', e.message));
  await page.goto(BASE + (hash ? '#' + hash : ''));
  await page.waitForTimeout(1500);
  return page;
}

async function settle(page) {
  await page.mouse.move(1, H - 1);
  await page.evaluate(() => {
    const a = document.activeElement;
    if (a instanceof HTMLElement && !a.closest('dialog, .popover')) a.blur();
  });
  await page.waitForTimeout(500);
}

/** The whole window. */
async function save(page, name) {
  await settle(page);
  await page.screenshot({ path: `${OUT}${name}.png` });
  console.log('saved', name);
  await page.context().close();
}

/**
 * A crop around the elements that match the selectors, with some room around them. With
 * `alone`, everything else on the page is hidden first, so the room shows the plain
 * background and not cut-off text behind a popup.
 */
async function crop(page, name, selectors, { pad = PAD, padLeft = pad, alone = false } = {}) {
  await settle(page);
  if (alone) {
    await page.evaluate((sels) => {
      const keep = sels.flatMap((s) => [...document.querySelectorAll(s)]);
      for (const e of document.querySelectorAll('body *')) e.style.visibility = 'hidden';
      const style = document.createElement('style');
      style.textContent = 'dialog::backdrop { background: transparent !important; }';
      document.head.append(style);
      for (const k of keep) {
        k.style.visibility = 'visible';
        for (const c of k.querySelectorAll('*')) c.style.visibility = 'visible';
      }
    }, selectors);
    await page.waitForTimeout(200);
  }
  const box = await page.evaluate((sels) => {
    const rects = sels.flatMap((s) => [...document.querySelectorAll(s)].map((e) => e.getBoundingClientRect()));
    if (!rects.length) return null;
    const x = Math.min(...rects.map((r) => r.left));
    const y = Math.min(...rects.map((r) => r.top));
    return {
      x,
      y,
      w: Math.max(...rects.map((r) => r.right)) - x,
      h: Math.max(...rects.map((r) => r.bottom)) - y
    };
  }, selectors);
  if (!box) throw new Error(`${name}: nothing matches ${selectors.join(', ')}`);
  const x = Math.max(0, box.x - padLeft);
  const y = Math.max(0, box.y - pad);
  const clip = {
    x,
    y,
    width: Math.min(W, box.x + box.w + (padLeft ? pad : 0)) - x,
    height: Math.min(page.viewportSize().height, box.y + box.h + pad) - y
  };
  await page.screenshot({ path: `${OUT}${name}.png`, clip });
  console.log('saved', name);
  await page.context().close();
}

const rail = (p, name) =>
  p.locator('nav.rail').getByRole('button', { name: new RegExp('^' + name) }).click();

/** Scroll the message list so that this element sits near its top. */
async function scrollTo(page, sel, offset = 8) {
  await page.evaluate(
    ([sel, offset]) => {
      const el = document.querySelector(sel);
      if (!el) return;
      let s = el.parentElement;
      while (s && !(s.scrollHeight > s.clientHeight && getComputedStyle(s).overflowY !== 'visible'))
        s = s.parentElement;
      if (!s) return;
      s.scrollTop += el.getBoundingClientRect().top - s.getBoundingClientRect().top - offset;
    },
    [sel, offset]
  );
  await page.waitForTimeout(300);
}

/**
 * Scroll the message list so that no message is cut at its top or bottom edge. Starts at
 * the newest messages and goes up. With `show`, that element must be fully in view too.
 */
async function frame(page, show = null) {
  // Link previews load when their message comes into view, and then the rows grow. Show
  // every row once and wait, so the heights below are the final ones.
  await page.evaluate(async () => {
    const list = document.querySelector('[id^="msg-"]')?.closest('.list');
    if (!list) return;
    list.style.scrollBehavior = 'auto';
    for (let y = 0; y < list.scrollHeight; y += list.clientHeight / 2) {
      list.scrollTop = y;
      await new Promise((r) => setTimeout(r, 300));
    }
  });
  await page.waitForTimeout(800);
  const ok = await page.evaluate((show) => {
    const first = document.querySelector('[id^="msg-"]');
    let s = first?.parentElement;
    while (s && !(s.scrollHeight > s.clientHeight && getComputedStyle(s).overflowY !== 'visible'))
      s = s.parentElement;
    if (!s) return false;
    // The list scrolls smoothly. Here each new position must apply at once.
    s.style.scrollBehavior = 'auto';
    const want = show ? document.querySelector(show) : null;
    // The composer and the typing line sit over the bottom of the list. The visible part
    // ends where the list stops being the top element.
    const box = s.getBoundingClientRect();
    const x = box.left + box.width / 2;
    let bottom = box.bottom - 1;
    while (bottom > box.top && !s.contains(document.elementFromPoint(x, bottom))) bottom--;
    let top = box.top + 1;
    while (top < bottom && !s.contains(document.elementFromPoint(x, top))) top++;
    // A message counts as cut when more than a few pixels of it are on each side of an edge.
    const cuts = (r, edge) => r.top < edge - 3 && r.bottom > edge + 3;
    // First look for a spot where no message is cut at either edge. If there is none, keep
    // the bottom clean: a cut message at the top reads as "more above", one at the bottom
    // looks broken.
    for (const both of [true, false]) {
      for (let y = s.scrollHeight - s.clientHeight; y >= 0; y -= 1) {
        s.scrollTop = y;
        // Ask for the rows each time: the list may render them again.
        const cut = [...s.querySelectorAll('[id^="msg-"]')].some((m) => {
          const r = m.getBoundingClientRect();
          return (both && cuts(r, top)) || cuts(r, bottom);
        });
        const r = want?.getBoundingClientRect();
        const seen = !r || (r.top >= top && r.bottom <= bottom);
        if (!cut && seen) return true;
      }
    }
    return false;
  }, show);
  if (!ok) console.warn('frame: no clean scroll position', show ?? '');
  await page.waitForTimeout(300);
}

const shots = {
  // Full window: basement / #music, the hero.
  async music(mode) {
    const p = await open('', mode);
    await frame(p, '#msg-m2');
    await save(p, `01-music-${mode}`);
  },
  async members(mode) {
    const p = await open('', mode, { members: true });
    await frame(p);
    await save(p, `02-music-members-${mode}`);
  },
  async home(mode) {
    const p = await open('', mode);
    await rail(p, 'Home and messages');
    await p.waitForTimeout(300);
    await p.getByText('jess', { exact: true }).first().click();
    await frame(p);
    await save(p, `03-home-${mode}`);
  },
  async login(mode) {
    const p = await open('login', mode);
    await save(p, `04-login-${mode}`);
  },
  // Crops.
  // The crops use a taller window at the same zoom, so the whole part fits.
  async preview(mode) {
    const p = await open('', mode, { h: 1000 });
    await frame(p, '#msg-m1');
    await scrollTo(p, '#msg-m1', 40);
    // A message row spans the whole list and has its own inner room at the sides.
    await crop(p, `05-link-preview-${mode}`, ['#msg-m1', '#msg-m2', '#msg-m6', '#msg-m7'], { padLeft: 0 });
  },
  async emoji(mode) {
    const p = await open('', mode);
    await p.getByRole('button', { name: 'Add an emoji' }).click();
    await p.waitForTimeout(800);
    await crop(p, `06-emoji-${mode}`, ['.popover'], { alone: true });
  },
  async join(mode) {
    const p = await open('', mode);
    await rail(p, 'Add a space');
    await p.getByRole('tab', { name: 'Join a space' }).click();
    await p.waitForTimeout(600);
    await crop(p, `07-join-${mode}`, ['dialog[open]'], { alone: true });
  },
  async profile(mode) {
    const p = await open('', mode);
    await p.locator('#msg-m5 button.name').click();
    await p.waitForTimeout(600);
    await crop(p, `08-profile-${mode}`, ['.popover'], { alone: true });
  },
  async appearance(mode) {
    const p = await open('settings/appearance', mode, { h: 900 });
    await p.evaluate(() => {
      const h = [...document.querySelectorAll('h1, h2')].find((e) => e.textContent?.trim() === 'Appearance');
      h?.setAttribute('data-shot', '');
    });
    await crop(p, `09-appearance-${mode}`, ['[data-shot]', '[role=radiogroup][aria-label$="theme"]']);
  },
  async pics(mode) {
    const p = await open('', mode, { h: 900 });
    await p.getByText('pics', { exact: true }).first().click();
    await p.waitForTimeout(800);
    await scrollTo(p, '#msg-i1', 24);
    await crop(p, `10-pics-${mode}`, ['#msg-i1 .gutter > *', '#msg-i1 .head > *', '#msg-i1 button.image'], { pad: 24, padLeft: 14 });
  }
};

for (const [name, fn] of Object.entries(shots)) {
  if (ONLY.length && !ONLY.includes(name)) continue;
  for (const mode of ['dark', 'light']) await fn(mode);
}
await browser.close();
