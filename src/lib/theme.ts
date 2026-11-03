// Automatic / Light / Dark. Saved for next time; Automatic
// follows Windows and changes live with it. The native title bar follows too.
import { backend } from './backend';

export type ThemeChoice = 'auto' | 'light' | 'dark';
const KEY = 'theme';
const media = matchMedia('(prefers-color-scheme: dark)');

export function savedTheme(): ThemeChoice {
  try {
    const v = localStorage.getItem(KEY);
    if (v === 'light' || v === 'dark' || v === 'auto') return v;
  } catch { /* storage may be unavailable */ }
  return 'auto';
}

function paint(choice: ThemeChoice) {
  const dark = choice === 'dark' || (choice === 'auto' && media.matches);
  document.documentElement.dataset.theme = dark ? 'dark' : 'light';
}

/** Sets the theme; `fade` cross-fades the whole window as one picture. */
export function applyTheme(choice: ThemeChoice, fade = false) {
  try { localStorage.setItem(KEY, choice); } catch { /* fine */ }
  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const doc = document as Document & { startViewTransition?: (cb: () => void) => unknown };
  if (fade && !reduced && doc.startViewTransition) doc.startViewTransition(() => paint(choice));
  else paint(choice);
  backend.then((b) => b.setTheme(choice === 'auto' ? null : choice)).catch(() => {});
}

media.addEventListener('change', () => {
  if (savedTheme() === 'auto') applyTheme('auto', true);
});
