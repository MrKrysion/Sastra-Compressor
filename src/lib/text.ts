// Numbers, sizes, and times the European way:
// 2,4 MB · 1.234 files · 64% · 19:30.
import { formatNumber } from './ui';

export const num = (n: number, digits?: number) =>
  Number.isFinite(n) ? formatNumber(n, digits) : '–';

/** Sizes as Windows Explorer counts them (1 KB = 1.024 bytes), so they match. */
export function bytes(n: number): string {
  if (n < 1024) return `${num(n)} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let v = n / 1024, u = 0;
  while (v >= 1024 && u < units.length - 1) { v /= 1024; u++; }
  // KB as whole numbers; MB and up with one decimal below 100 (2,4 MB, 412 MB).
  return `${num(v, u === 0 || v >= 100 ? 0 : 1)} ${units[u]}`;
}

export const percent = (fraction: number) => `${num(Math.round(fraction * 100))}%`;

export function plural(n: number, one: string, many = one + 's') {
  return `${num(n)} ${n === 1 ? one : many}`;
}

/** "2 min 13 s left", counted down every second like a download. */
export function timeLeft(seconds: number): string {
  if (!isFinite(seconds) || seconds < 0) return '';
  const t = Math.max(1, Math.round(seconds));
  const h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), s = t % 60;
  if (h) return `${num(h)} h ${num(m)} min left`;
  if (m) return `${num(m)} min ${num(s)} s left`;
  return `${num(s)} s left`;
}

/** A list in words, with the Oxford comma: "PNG, JPEG, and WebP". */
export function list(words: string[]): string {
  if (words.length < 3) return words.join(' and ');
  return `${words.slice(0, -1).join(', ')}, and ${words[words.length - 1]}`;
}
