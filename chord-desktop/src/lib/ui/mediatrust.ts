// Checks on the remote URLs of messages: only http and https, the real host to show, and
// no request by itself to a private or local address. The "Show link previews" switch in
// the privacy settings turns embeds off.

/** The URL of a remote file if it is a plain http or https URL, else null. */
export function webUrl(url: string): URL | null {
  try {
    const u = new URL(url);
    if (u.protocol !== 'https:' && u.protocol !== 'http:') return null;
    return u;
  } catch {
    return null;
  }
}

/** The host to show next to a file: the real host of the URL, never text of the sender. */
export function hostOf(url: string): string {
  return webUrl(url)?.hostname ?? '';
}

function isPrivateV4(a: number, b: number): boolean {
  return (
    a === 0 ||
    a === 10 ||
    a === 127 ||
    (a === 100 && b >= 64 && b <= 127) ||
    (a === 169 && b === 254) ||
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 168) ||
    a >= 224
  );
}

function isLocalV6(ip: string): boolean {
  const s = ip.toLowerCase();
  if (s === '::' || s === '::1') return true;
  if (s.startsWith('::ffff:') || s.startsWith('64:ff9b:')) return true;
  return /^(fe[89ab]|f[cd])/.test(s);
}

/**
 * True if the URL is no public web URL by its text: a scheme other than http(s),
 * `localhost`, a name with no dot or a `.local` name, or an IP literal in a private,
 * loopback or link-local range. The webview does no other check, so the UI never loads
 * such a URL by itself. A name that resolves to a private address is out of reach here.
 * The Rust side checks the addresses of its own fetches.
 */
export function isLocalHost(url: string): boolean {
  const u = webUrl(url);
  if (!u) return true;
  const host = u.hostname.toLowerCase().replace(/\.$/, '');
  if (host.startsWith('[')) return isLocalV6(host.slice(1, -1));
  if (host === 'localhost' || host.endsWith('.localhost')) return true;
  if (host.endsWith('.local') || host.endsWith('.internal') || !host.includes('.')) return true;
  const v4 = host.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/);
  if (v4) return isPrivateV4(Number(v4[1]), Number(v4[2]));
  // A number host such as 0x7f.1 is an address in disguise. The URL parser turns the
  // common forms into dotted decimals, so what is left and looks like a number is refused.
  return /^[\d.x]+$/i.test(host);
}
