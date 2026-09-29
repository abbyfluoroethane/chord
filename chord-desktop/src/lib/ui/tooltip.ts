// Tooltip action. The tip lives on document.body so scroll containers do not clip it.
// Usage: use:tooltip={'Settings'} or use:tooltip={{ text: 'Settings', side: 'bottom' }}

type Side = 'right' | 'bottom' | 'top';
type Param = string | { text: string; side?: Side } | null | undefined;

function read(p: Param): { text: string; side: Side } {
  if (!p) return { text: '', side: 'right' };
  return typeof p === 'string'
    ? { text: p, side: 'right' }
    : { text: p.text, side: p.side ?? 'right' };
}

export function tooltip(node: HTMLElement, param: Param) {
  let cfg = read(param);
  let tip: HTMLDivElement | null = null;

  function show() {
    if (!cfg.text || tip) return;
    tip = document.createElement('div');
    tip.className = 'chord-tooltip';
    tip.setAttribute('role', 'tooltip');
    tip.textContent = cfg.text;
    document.body.appendChild(tip);
    const r = node.getBoundingClientRect();
    const t = tip.getBoundingClientRect();
    let x = r.right + 12;
    let y = r.top + (r.height - t.height) / 2;
    if (cfg.side === 'bottom') {
      x = r.left + (r.width - t.width) / 2;
      y = r.bottom + 8;
    } else if (cfg.side === 'top') {
      x = r.left + (r.width - t.width) / 2;
      y = r.top - t.height - 8;
    }
    x = Math.max(8, Math.min(x, window.innerWidth - t.width - 8));
    y = Math.max(8, Math.min(y, window.innerHeight - t.height - 8));
    tip.style.left = `${x}px`;
    tip.style.top = `${y}px`;
  }

  function hide() {
    tip?.remove();
    tip = null;
  }

  node.addEventListener('pointerenter', show);
  node.addEventListener('pointerleave', hide);
  node.addEventListener('focus', show);
  node.addEventListener('blur', hide);
  node.addEventListener('pointerdown', hide);

  return {
    update(next: Param) {
      cfg = read(next);
      if (tip) {
        hide();
        show();
      }
    },
    destroy() {
      hide();
      node.removeEventListener('pointerenter', show);
      node.removeEventListener('pointerleave', hide);
      node.removeEventListener('focus', show);
      node.removeEventListener('blur', hide);
      node.removeEventListener('pointerdown', hide);
    }
  };
}
