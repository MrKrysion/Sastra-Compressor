// The interface's own behaviour: tooltips, the sliding pill in segmented controls, and toasts.

const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches;
const EASE = 'cubic-bezier(.2, .8, .2, 1)';
const SPRING = 'cubic-bezier(.34, 1.36, .64, 1)';
const viewportWidth = () => document.documentElement.clientWidth || innerWidth;

/* ---------- tooltips ---------- */
// Any [data-tip] (or [title], which is taken over so the browser's own tooltip never shows).
// Labelled buttons only show it when their text is cut off; data-tip-always shows it anyway.

let tip: HTMLElement | null = null, tipTimer = 0, tipGoneAt = 0;

function showTip(target: Element) {
  clearTimeout(tipTimer);
  // Moving from one tip straight to the next shows it at once.
  const instant = !!tip || Date.now() - tipGoneAt < 300;
  const go = () => {
    hideTip(true);
    const text = target.getAttribute('data-tip'); if (!text) return;
    const t = tip = document.createElement('div');
    t.className = 'tooltip' + (instant ? ' instant show' : ''); t.setAttribute('role', 'tooltip'); t.textContent = text;
    document.body.appendChild(t);
    const r = target.getBoundingClientRect(), w = t.offsetWidth, h = t.offsetHeight;
    let top = r.top - h - 8; if (top < 8) top = r.bottom + 8;
    t.style.left = Math.max(8, Math.min(viewportWidth() - w - 8, r.left + r.width / 2 - w / 2)) + 'px';
    t.style.top = top + 'px';
    if (!instant) requestAnimationFrame(() => tip && tip.classList.add('show'));
  };
  if (instant) go(); else tipTimer = window.setTimeout(go, 400);
}

function hideTip(quiet = false) {
  clearTimeout(tipTimer);
  if (tip) { tip.remove(); tip = null; if (!quiet) tipGoneAt = Date.now(); }
}

const TITLED = '[title]:not(iframe, abbr, svg, svg *, link, style, meta)';
function adopt(el: EventTarget | null) {
  const t = el instanceof Element ? el.closest(TITLED) : null; if (!t) return;
  const v = t.getAttribute('title'); t.removeAttribute('title'); if (!v) return;
  t.setAttribute('data-tip', v);
  if (!t.hasAttribute('aria-label') && !t.textContent?.trim()) t.setAttribute('aria-label', v);
}

function needsTip(t: HTMLElement) {
  if (t.hasAttribute('data-tip-always')) return true;
  return !(t.innerText || '').trim() || t.scrollWidth > t.clientWidth + 1;
}

const tipTarget = (el: EventTarget | null) => (el instanceof Element ? el.closest<HTMLElement>('[data-tip]') : null);

function initTooltips() {
  document.addEventListener('pointerover', (e) => {
    adopt(e.target); if (e.buttons) return;
    const t = tipTarget(e.target);
    if (t && e.pointerType !== 'touch' && needsTip(t)) showTip(t);
  });
  document.addEventListener('pointerout', (e) => {
    const t = tipTarget(e.target);
    if (t && !t.contains(e.relatedTarget as Node | null)) hideTip();
  });
  document.addEventListener('focusin', (e) => {
    adopt(e.target);
    const t = tipTarget(e.target);
    if (t && t.matches(':focus-visible') && needsTip(t)) showTip(t);
  });
  document.addEventListener('focusout', () => hideTip());
  document.addEventListener('pointerdown', () => hideTip(true));
  addEventListener('scroll', () => hideTip(true), { passive: true, capture: true });
}

/* ---------- segmented controls ---------- */
// A pill under the chosen button that stretches across to the next choice, then settles.

const ON = 'button.on, button[aria-selected="true"], button[aria-pressed="true"]';
const ON_CHILD = ':scope > ' + ON.split(', ').join(', :scope > ');
const segsDone = new WeakSet<Element>();

function initSeg(seg: HTMLElement) {
  if (segsDone.has(seg) || seg.hasAttribute('data-static')) return;
  segsDone.add(seg);
  seg.classList.add('seg-slide');
  const ind = document.createElement('span'); ind.className = 'seg-ind'; ind.setAttribute('aria-hidden', 'true'); seg.prepend(ind);
  let cur: HTMLElement | null = null;
  const pos = (b: HTMLElement) => ({ x: b.offsetLeft, y: b.offsetTop, w: b.offsetWidth, h: b.offsetHeight });
  const place = (b: HTMLElement | null) => {
    if (!b) { ind.style.opacity = '0'; return; }
    const p = pos(b);
    ind.style.opacity = '1'; ind.style.width = p.w + 'px'; ind.style.height = p.h + 'px'; ind.style.transform = `translate(${p.x}px, ${p.y}px)`;
  };
  const sync = () => {
    // React may re-render the control's children and drop the pill.
    if (!ind.isConnected) { seg.prepend(ind); cur = null; }
    const b = seg.querySelector<HTMLElement>(ON_CHILD);
    if (b === cur) { place(b); return; }
    const from = cur && cur.isConnected ? pos(cur) : null; cur = b; place(b);
    if (!from || !b || reduced()) return;
    const to = pos(b);
    if (from.y !== to.y) { ind.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 150 }); return; }
    const L = Math.min(from.x, to.x), R = Math.max(from.x + from.w, to.x + to.w);
    ind.animate([
      { transform: `translate(${from.x}px, ${to.y}px)`, width: from.w + 'px' },
      { transform: `translate(${L}px, ${to.y}px)`, width: (R - L) + 'px', offset: .45 },
      { transform: `translate(${to.x}px, ${to.y}px)`, width: to.w + 'px' },
    ], { duration: 320, easing: EASE });
  };
  new MutationObserver(sync).observe(seg, { subtree: true, attributes: true, attributeFilter: ['class', 'aria-selected', 'aria-pressed'], childList: true });
  new ResizeObserver(() => place(cur)).observe(seg);
  sync();
}

function initSegs() {
  const all = () => document.querySelectorAll<HTMLElement>('.seg').forEach(initSeg);
  let queued = 0;
  new MutationObserver((muts) => {
    if (queued) return;
    for (const m of muts) for (const n of m.addedNodes) {
      if (n instanceof Element && (n.matches('.seg') || n.querySelector('.seg'))) { queued = window.setTimeout(() => { queued = 0; all(); }); return; }
    }
  }).observe(document.body, { childList: true, subtree: true });
  all();
}

/** Sets up tooltips and segmented controls; call once, before the app renders. */
export function initUi() {
  initTooltips();
  initSegs();
}

/* ---------- toasts ---------- */

export type ToastKind = 'info' | 'ok' | 'err' | 'warn';
const ICONS: Record<ToastKind, string> = { info: 'fa-circle-info', ok: 'fa-circle-check', err: 'fa-circle-exclamation', warn: 'fa-triangle-exclamation' };
type Toast = HTMLElement & { _restart?: () => void };

function host() {
  let h = document.querySelector<HTMLElement>('.toast-host');
  if (!h) {
    h = document.createElement('div'); h.className = 'toast-host'; h.setAttribute('role', 'status'); h.setAttribute('aria-live', 'polite');
    document.body.appendChild(h);
  }
  return h;
}

/** Runs `change`, then slides the toasts that moved from where they were. */
function glide(h: HTMLElement, change: () => void) {
  const before = new Map([...h.children].map((k) => [k, k.getBoundingClientRect().top]));
  change();
  if (reduced()) return;
  for (const k of h.children) {
    if (!before.has(k)) continue;
    const dy = before.get(k)! - k.getBoundingClientRect().top;
    if (dy) k.animate([{ transform: `translateY(${dy}px)` }, { transform: 'none' }], { duration: 240, easing: EASE });
  }
}

/** Marks a toast whose text wraps, so it can lay out for more than one line. */
function fit(t: HTMLElement) {
  const s = t.querySelector('span')!;
  t.classList.toggle('multi', s.getBoundingClientRect().height > parseFloat(getComputedStyle(s).lineHeight) * 1.5);
}

/** Shows a toast. The same message again counts up (×2) instead of stacking; at most three show. */
export function toast(message: string, kind: ToastKind = 'info') {
  const h = host(), life = kind === 'err' ? 5000 : 3000;
  const dup = [...h.querySelectorAll<Toast>('.toast:not(.leaving)')].find((t) => t.dataset.msg === message && t.dataset.kind === kind);
  if (dup) {
    const n = (+(dup.dataset.n ?? 1) || 1) + 1; dup.dataset.n = String(n);
    let c = dup.querySelector('.toast-count');
    if (!c) { c = document.createElement('span'); c.className = 'toast-count'; dup.appendChild(c); }
    c.textContent = '×' + n; fit(dup);
    if (!reduced()) dup.animate([{ transform: 'scale(1)' }, { transform: 'scale(1.04)' }, { transform: 'scale(1)' }], { duration: 220, easing: SPRING });
    dup._restart?.();
    return;
  }
  const t: Toast = document.createElement('div');
  t.className = 'toast' + (kind === 'info' ? '' : ' ' + kind); t.dataset.msg = message; t.dataset.kind = kind;
  t.innerHTML = `<i class="fa-solid ${ICONS[kind]}" aria-hidden="true"></i><span></span>`;
  t.querySelector('span')!.textContent = message;
  // Hovering pauses the countdown; leaving resumes it.
  let timer = 0, left = life, since = 0;
  const run = () => { since = Date.now(); timer = window.setTimeout(leave, left); };
  function leave() {
    if (t.classList.contains('leaving')) return;
    clearTimeout(timer); t.classList.add('leaving');
    setTimeout(() => glide(h, () => t.remove()), 200);
  }
  t._restart = () => { clearTimeout(timer); left = life; run(); };
  t.addEventListener('mouseenter', () => { clearTimeout(timer); left -= Date.now() - since; });
  t.addEventListener('mouseleave', run);
  t.addEventListener('click', leave);
  glide(h, () => { h.prepend(t); fit(t); });
  [...h.querySelectorAll('.toast:not(.leaving)')].slice(3).forEach((o) => { o.classList.add('leaving'); setTimeout(() => o.remove(), 200); });
  run();
}

/* ---------- numbers ---------- */

/** Numbers the European way: 2,4 and 1.234. */
export function formatNumber(n: number, digits?: number) {
  const o = digits == null ? { maximumFractionDigits: 2 } : { minimumFractionDigits: digits, maximumFractionDigits: digits };
  return new Intl.NumberFormat('id-ID', o).format(n);
}
