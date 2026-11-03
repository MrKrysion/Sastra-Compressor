export type Kind = 'image' | 'audio' | 'video';
export type Level = 'smaller' | 'balanced' | 'sharper';
export type SaveMode = 'beside' | 'replace' | 'folder';

/** A file as the backend found it (src-tauri/src/scan.rs). */
export interface Item {
  path: string;
  name: string;
  /** Path inside the folder it was added from, or the name alone. */
  rel: string;
  kind: Kind;
  size: number;
}

export interface ScanResult {
  items: Item[];
  unsupported: string[];
}

export interface Formats {
  image: string[];
  audio: string[];
  video: string[];
}

/** Sent to the backend as is (src-tauri/src/settings.rs). */
export interface Settings {
  imageQuality: Level;
  imageMaxWidth: number;
  audioBitrate: number;
  videoQuality: Level;
  videoMaxWidth: number;
  saveMode: SaveMode;
  /** Used when saveMode is 'folder'. */
  outputFolder: string | null;
}

export type JobUpdate =
  | { state: 'running'; id: string; progress: number }
  | { state: 'done'; id: string; outPath: string; outSize: number }
  | { state: 'failed'; id: string; error: string }
  | { state: 'cancelled'; id: string };

export type RowState =
  | { state: 'waiting' }
  | { state: 'running'; progress: number }
  /** `replaced`: the original went to the Recycle Bin, so this file can't be done again. */
  | { state: 'done'; outPath: string; outSize: number; replaced?: boolean }
  | { state: 'failed'; error: string };

export interface Row extends Item {
  status: RowState;
}
