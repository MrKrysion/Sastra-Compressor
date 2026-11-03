import type { Settings } from './types';

const KEY = 'settings';

export const defaults: Settings = {
  imageQuality: 'balanced',
  imageMaxWidth: 0,
  audioBitrate: 96,
  videoQuality: 'balanced',
  videoMaxWidth: 0,
  saveMode: 'beside',
  outputFolder: null,
};

export function loadSettings(): Settings {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) || '{}');
    const s: Settings = { ...defaults, ...v };
    // Settings saved before "Replace originals" existed: a folder meant folder mode.
    if (!v.saveMode && v.outputFolder) s.saveMode = 'folder';
    if (s.saveMode === 'folder' && !s.outputFolder) s.saveMode = 'beside';
    return s;
  } catch {
    return defaults;
  }
}

export function saveSettings(s: Settings) {
  try { localStorage.setItem(KEY, JSON.stringify(s)); } catch { /* fine */ }
}
