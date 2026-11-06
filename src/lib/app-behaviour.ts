// A webview is a browser underneath. Without this, F5 or Ctrl+R reloads the page and
// empties the file list mid-run, Ctrl+P prints the window, and right-clicking offers
// "Back" and "Refresh". A desktop app does none of that.
import { inApp } from './backend';

const canEditText = (t: EventTarget | null) =>
  t instanceof HTMLElement && (t.isContentEditable || t.matches('input, textarea, select'));

const BLOCKED_ALONE = new Set(['F3', 'F5', 'F7']);
// Print, find, reload, view source, save, open a new window, go to the address bar, and so on.
const BLOCKED_WITH_CTRL = new Set(['p', 'f', 'g', 'r', 'u', 's', 'n', 't', 'l', 'j', 'h', '+', '=', '-', '0']);

export function behaveLikeAnApp() {
  // In a plain browser (`npm run dev`) the developer wants all of these.
  if (!inApp) return;

  document.addEventListener('keydown', (e) => {
    const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    if (BLOCKED_ALONE.has(key) && !e.altKey) e.preventDefault();
    if ((e.ctrlKey || e.metaKey) && BLOCKED_WITH_CTRL.has(key) && !canEditText(e.target)) e.preventDefault();
    if (e.altKey && (key === 'ArrowLeft' || key === 'ArrowRight')) e.preventDefault();
    if (import.meta.env.PROD && (key === 'F12' || (e.ctrlKey && e.shiftKey && (key === 'i' || key === 'j' || key === 'c')))) e.preventDefault();
  });

  // Copying selected text and editing a field keep their menu; nothing else has one.
  document.addEventListener('contextmenu', (e) => {
    if (canEditText(e.target) || !!window.getSelection()?.toString()) return;
    e.preventDefault();
  });

  // The logo and icons shouldn't peel off and drag like web images.
  document.addEventListener('dragstart', (e) => e.preventDefault());

  // The mouse's back and forward buttons.
  document.addEventListener('mouseup', (e) => { if (e.button === 3 || e.button === 4) e.preventDefault(); });
  // Ctrl+wheel zoom would put the maximise button's native target out of step.
  document.addEventListener('wheel', (e) => { if (e.ctrlKey) e.preventDefault(); }, { passive: false });
}
