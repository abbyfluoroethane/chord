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

/** Compact and dense: the look the shots use. */
const PREFS = JSON.stringify({ display: 'compact', fontSize: 14 });

/**
 * The main shots are 2880x1800 PNGs: a 1440x900 window at 2x, with the app at about 88 %
 * zoom, so the whole of #playtest fits. The preview's own zoom (Settings, Appearance, Zoom)
 * is CSS zoom, which leaves a gap at the bottom, so the page gets a 1640x1025 viewport at a
 * scale of 1.756 instead. That is the
 * same picture as the webview zoom of the app.
 */
async function open(hash, mode, { w = 1640, h = 1025, scale = 2880 / 1640, members = true } = {}) {
  const ctx = await browser.newContext({
    viewport: { width: w, height: h },
    deviceScaleFactor: scale,
    colorScheme: mode
  });
  await ctx.addInitScript(
    ([prefs, mode, members]) => {
      localStorage.setItem('chord.prefs', prefs);
      localStorage.setItem('chord.theme', mode);
      localStorage.setItem('chord.membersOpen', members ? '1' : '0');
    },
    [PREFS, mode, members]
  );
  const page = await ctx.newPage();
  page.on('pageerror', (e) => console.error('pageerror', e.message));
  await page.goto(BASE + (hash ? '#' + hash : ''));
  await page.waitForTimeout(1200);
  return page;
}

async function save(page, name) {
  await page.mouse.move(2, 2);
  await page.waitForTimeout(500);
  await page.screenshot({ path: `${OUT}${name}.png` });
  console.log('saved', name);
  await page.context().close();
}

/** Scroll the message list so that the element that matches sel sits near its top. */
async function scrollTo(page, sel, offset = 10) {
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

const rail = (p, name) => p.locator('nav.rail').getByRole('button', { name: new RegExp('^' + name) }).click();

const shots = {
  async main(mode) {
    const p = await open('', mode);
    await save(p, `01-main-${mode}`);
  },
  async home(mode) {
    const p = await open('', mode);
    await rail(p, 'Home and messages');
    await p.waitForTimeout(300);
    await p.getByText('Theo Lindqvist', { exact: true }).first().click();
    await save(p, `02-home-${mode}`);
  },
  async darkroom(mode) {
    const p = await open('', mode);
    await rail(p, 'Darkroom');
    await p.waitForTimeout(300);
    await p.getByText('show-and-tell', { exact: true }).first().click();
    await p.waitForTimeout(600);
    await save(p, `03-darkroom-${mode}`);
  },
  async browse(mode) {
    const p = await open('', mode);
    await rail(p, 'Add a space');
    await p.getByRole('tab', { name: 'Join a space' }).click();
    await p.waitForTimeout(600);
    await save(p, `04-browse-${mode}`);
  },
  async profile(mode) {
    const p = await open('', mode);
    await p.locator('aside').getByText('Priya Raman', { exact: true }).first().click();
    await p.waitForTimeout(500);
    await save(p, `05-profile-${mode}`);
  },
  async appearance(mode) {
    const p = await open('settings/appearance', mode);
    // The dialog puts the focus on its first item. The focus ring is not part of the picture.
    await p.evaluate(() => (document.activeElement)?.blur());
    await save(p, `06-appearance-${mode}`);
  },
  async emoji(mode) {
    if (mode !== 'dark') return;
    const p = await open('', mode);
    await p.getByRole('button', { name: 'Add an emoji' }).click();
    await p.waitForTimeout(800);
    await save(p, `07-emoji-${mode}`);
  },
  async login(mode) {
    const p = await open('login', mode);
    await save(p, `08-login-${mode}`);
  },
  async small(mode) {
    const p = await open('', mode, { w: 1100, h: 700, scale: 2, members: false });
    await save(p, `09-compact-small-${mode}`);
  }
};
for (const [name, fn] of Object.entries(shots)) {
  if (ONLY.length && !ONLY.includes(name)) continue;
  for (const mode of ['dark', 'light']) await fn(mode);
}
await browser.close();
