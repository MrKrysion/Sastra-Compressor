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
  /** Found in a folder and named like this app's own output. */
  compressed: boolean;
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
  /** 0 keeps the frame rate. */
  videoMaxFps: number;
  saveMode: SaveMode;
  /** Used when saveMode is 'folder'. */
  outputFolder: string | null;
}

/** Answers from the confirmation dialogs; they apply to one run and are never saved. */
export interface RunChoices {
  /** Give new files a number rather than overwrite what's there. */
  keepExisting?: boolean;
  /** Originals with no Recycle Bin to go to are deleted. */
  deleteForGood?: boolean;
}

/** Something worth saying about a finished file. `warn` marks something lost. */
export interface Note {
  text: string;
  warn: boolean;
}

export type JobUpdate =
  | { state: 'running'; id: string; progress: number }
  | { state: 'done'; id: string; outPath: string; outSize: number; kept: boolean; note: Note | null }
  | { state: 'failed'; id: string; error: string }
  | { state: 'cancelled'; id: string };

export type RowState =
  | { state: 'waiting' }
  | { state: 'running'; progress: number }
  /**
   * `replaced`: the original is gone from its place, so this file can't be done again.
   * `kept`: replacing, but the result wasn't smaller, so the original stayed.
   */
  | { state: 'done'; outPath: string; outSize: number; replaced: boolean; kept: boolean; note: Note | null }
  | { state: 'failed'; error: string };

export interface Row extends Item {
  status: RowState;
}
