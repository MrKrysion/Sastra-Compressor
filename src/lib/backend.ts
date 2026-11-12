// Everything the interface asks of the app. Inside Tauri it talks to the
// Rust side; in a plain browser (`npm run dev`) a stand-in fakes the files
// and the compressing, so the design can be worked on without building.
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { FormatInfo, Formats, JobUpdate, Kind, RunChoices, ScanResult, Settings } from './types';

export const inApp = isTauri();

/** What the title bar draws differently (src-tauri/src/frame.rs). */
export interface FrameState {
  maximized: boolean;
  active: boolean;
  /** The pointer is over the maximise button, which the interface can't see for itself. */
  hover: boolean;
  pressed: boolean;
}

export interface Job {
  id: string;
  path: string;
  rel: string;
  kind: Kind;
}

interface Backend {
  formats(): Promise<Formats>;
  scan(paths: string[]): Promise<ScanResult>;
  /** Where each job will be saved, in the same order. */
  plan(jobs: Job[], settings: Settings): Promise<string[]>;
  compress(jobs: Job[], settings: Settings & RunChoices): Promise<void>;
  cancel(): Promise<void>;
  /** Files already on disk that compressing these jobs would overwrite. */
  existing(jobs: Job[], settings: Settings): Promise<string[]>;
  /** Which of these files Windows wouldn't put in the Recycle Bin. */
  noBin(paths: string[]): Promise<string[]>;
  version(): Promise<string>;
  onJob(fn: (u: JobUpdate) => void): Promise<() => void>;
  onFinished(fn: (cancelled: boolean) => void): Promise<() => void>;
  pickFiles(f: Formats): Promise<string[]>;
  pickFolders(): Promise<string[]>;
  pickOutputFolder(): Promise<string | null>;
  reveal(path: string): Promise<void>;
  openUrl(url: string): Promise<void>;
  onDrop(fn: (e: { type: 'over' | 'leave' | 'drop'; paths: string[] }) => void): Promise<() => void>;
  setTheme(t: 'light' | 'dark' | null): Promise<void>;
  setProgress(fraction: number | null): Promise<void>;
  notify(title: string, body: string): Promise<void>;
  onCloseRequested(fn: () => boolean): Promise<() => void>;
  quit(): Promise<void>;
  /** The title bar's window controls. */
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  frameState(): Promise<FrameState | null>;
  onFrame(fn: (s: FrameState) => void): Promise<() => void>;
  setMaximizeButton(right: number, width: number, height: number): Promise<void>;
  systemMenu(fromKeyboard: boolean): Promise<void>;
  setBackground(color: string): Promise<void>;
  show(): Promise<void>;
}

async function tauriBackend(): Promise<Backend> {
  const { listen } = await import('@tauri-apps/api/event');
  const { getCurrentWindow, ProgressBarStatus } = await import('@tauri-apps/api/window');
  const { getCurrentWebview } = await import('@tauri-apps/api/webview');
  const dialog = await import('@tauri-apps/plugin-dialog');
  const opener = await import('@tauri-apps/plugin-opener');
  const win = getCurrentWindow();
  const asList = (x: string | string[] | null) => (x == null ? [] : Array.isArray(x) ? x : [x]);

  return {
    formats: () => invoke('formats'),
    scan: (paths) => invoke('scan', { paths }),
    plan: (jobs, settings) => invoke('plan', { jobs, settings }),
    compress: (jobs, settings) => invoke('compress', { jobs, settings }),
    cancel: () => invoke('cancel'),
    existing: (jobs, settings) => invoke('existing', { jobs, settings }),
    noBin: (paths) => invoke('no_bin', { paths }),
    version: () => invoke('version'),
    onJob: (fn) => listen<JobUpdate>('job', (e) => fn(e.payload)),
    onFinished: (fn) => listen<{ cancelled: boolean }>('finished', (e) => fn(e.payload.cancelled)),
    pickFiles: async (f) =>
      asList(
        await dialog.open({
          multiple: true,
          title: 'Add files',
          filters: [
            { name: 'Images, audio, and video', extensions: [...f.image, ...f.audio, ...f.video] },
            { name: 'Images', extensions: f.image },
            { name: 'Audio', extensions: f.audio },
            { name: 'Video', extensions: f.video },
          ],
        }),
      ),
    pickFolders: async () => asList(await dialog.open({ directory: true, multiple: true, title: 'Add folders' })),
    pickOutputFolder: async () => {
      const r = await dialog.open({ directory: true, multiple: false, title: 'Save compressed files to' });
      return typeof r === 'string' ? r : null;
    },
    reveal: (path) => opener.revealItemInDir(path),
    openUrl: (url) => opener.openUrl(url),
    onDrop: (fn) =>
      getCurrentWebview().onDragDropEvent((e) => {
        const p = e.payload;
        if (p.type === 'enter' || p.type === 'over') fn({ type: 'over', paths: [] });
        else if (p.type === 'drop') fn({ type: 'drop', paths: p.paths });
        else fn({ type: 'leave', paths: [] });
      }),
    setTheme: (t) => win.setTheme(t),
    setProgress: (fraction) =>
      win.setProgressBar(
        fraction == null
          ? { status: ProgressBarStatus.None }
          : { status: ProgressBarStatus.Normal, progress: Math.round(fraction * 100) },
      ),
    // Like the old app: always, so it's there even after looking away. The Rust side
    // sends it, so it can carry the logo (src-tauri/src/windows.rs).
    notify: (title, body) => invoke('notify', { title, body }),
    onCloseRequested: (fn) =>
      win.onCloseRequested((e) => {
        if (!fn()) e.preventDefault();
      }),
    quit: () => invoke('quit'),
    minimize: () => win.minimize(),
    toggleMaximize: () => win.toggleMaximize(),
    close: () => win.close(),
    frameState: () => invoke('frame_state'),
    onFrame: (fn) => listen<FrameState>('frame', (e) => fn(e.payload)),
    setMaximizeButton: (right, width, height) => invoke('set_maximize_button', { right, width, height }),
    systemMenu: (fromKeyboard) => invoke('system_menu', { fromKeyboard }),
    setBackground: (color) => win.setBackgroundColor(color),
    show: () => win.show(),
  };
}

/* ---------- the browser stand-in ---------- */
function mockBackend(): Backend {
  // A shortened copy of src-tauri/src/formats.rs, as Windows sees it.
  const f = (kind: Kind, name: string, ext: string, common: string | null = null, note: string | null = null): FormatInfo => ({ kind, name, ext: ext.split(' '), common, note });
  const list: FormatInfo[] = [
    f('image', 'PNG', 'png', 'PNG'), f('image', 'JPEG', 'jpg jpeg', 'JPEG'), f('image', 'WebP', 'webp', 'WebP'), f('image', 'Photoshop', 'psd', 'PSD'),
    f('image', 'GIF', 'gif'), f('image', 'AVIF', 'avif'), f('image', 'JPEG XL', 'jxl'),
    f('image', 'HEIF and HEIC', 'heic heif', 'HEIC', 'Needs Microsoft’s HEIF and HEVC extensions'),
    f('audio', 'WAV', 'wav', 'WAV'), f('audio', 'MP3', 'mp3', 'MP3'), f('audio', 'FLAC', 'flac', 'FLAC'), f('audio', 'Ogg Vorbis and Opus', 'ogg oga opus', 'OGG'),
    f('audio', 'AAC and Apple Lossless', 'm4a m4b aac', 'M4A'), f('audio', 'Windows Media Audio', 'wma'), f('audio', 'AMR', 'amr 3ga'),
    f('video', 'MP4', 'mp4 m4v', 'MP4'), f('video', 'QuickTime', 'mov', 'MOV'), f('video', 'Matroska', 'mkv', 'MKV'), f('video', 'WebM', 'webm', 'WebM'),
    f('video', 'AVI', 'avi', 'AVI'), f('video', 'Windows Media Video', 'wmv asf'), f('video', 'AVCHD and MPEG-TS', 'mts m2ts ts'),
  ];
  const ext = (k: Kind) => list.filter((x) => x.kind === k).flatMap((x) => x.ext);
  const formats: Formats = {
    image: ext('image'), audio: ext('audio'), video: ext('video'), list,
    videoNote: 'HEVC (H.265) video needs Microsoft’s HEVC Video Extensions. ProRes and DNxHD aren’t supported: export those as H.264.',
  };
  const samples: [string, Kind, number][] = [
    ['bg/school_hallway.png', 'image', 6_811_000],
    ['bg/rooftop_sunset.png', 'image', 7_402_000],
    ['bg/rooftop_sunset_compressed.webp', 'image', 702_000],
    ['bg/sparkles.gif', 'image', 3_120_000],
    ['sprites/erika_smile.png', 'image', 2_310_000],
    ['sprites/erika_angry.png', 'image', 2_287_000],
    ['music/main_theme.wav', 'audio', 52_480_000],
    ['voice/erika_0001.wav', 'audio', 412_000],
    ['sfx/door_slam.flac', 'audio', 188_000],
    ['video/opening.mov', 'video', 412_600_000],
  ];
  const jobFns: ((u: JobUpdate) => void)[] = [];
  const doneFns: ((c: boolean) => void)[] = [];
  const dropFns: ((e: { type: 'over' | 'leave' | 'drop'; paths: string[] }) => void)[] = [];
  let timers: number[] = [];
  let cancelled = false;

  window.addEventListener('dragover', (e) => { e.preventDefault(); dropFns.forEach((f) => f({ type: 'over', paths: [] })); });
  window.addEventListener('dragleave', (e) => { if (!e.relatedTarget) dropFns.forEach((f) => f({ type: 'leave', paths: [] })); });
  window.addEventListener('drop', (e) => { e.preventDefault(); dropFns.forEach((f) => f({ type: 'drop', paths: ['C:/VN/assets'] })); });

  return {
    formats: async () => formats,
    scan: async (paths) => ({
      items: paths.flatMap((p) =>
        samples.map(([rel, kind, size]) => ({
          path: `${p}/${rel}`, name: rel.split('/').pop()!, rel, kind, size, compressed: rel.includes('_compressed'),
        })),
      ),
      unsupported: ['notes.txt'],
    }),
    plan: async (jobs, s) => {
      const ext = { image: 'webp', audio: 'ogg', video: 'webm' } as const;
      return jobs.map((j) => {
        const dir = j.path.slice(0, j.path.lastIndexOf('/'));
        const stem = j.path.slice(dir.length + 1).replace(/\.[^.]+$/, '');
        if (s.saveMode === 'replace') return `${dir}/${stem}.${ext[j.kind]}`;
        if (s.saveMode === 'folder' && s.outputFolder) return `${s.outputFolder}/${j.rel.replace(/\.[^./]+$/, '')}.${ext[j.kind]}`;
        return `${dir}/${stem}_compressed.${ext[j.kind]}`;
      });
    },
    compress: async (jobs) => {
      cancelled = false;
      const ratio: Record<Kind, number> = { image: 0.18, audio: 0.09, video: 0.12 };
      let at = 0;
      jobs.forEach((j, i) => {
        const steps = j.kind === 'video' ? 40 : 6;
        for (let s = 0; s <= steps; s++) {
          timers.push(window.setTimeout(() => {
            jobFns.forEach((f) => f(s < steps
              ? { state: 'running', id: j.id, progress: s / steps }
              : j.rel.includes('door')
                ? { state: 'failed', id: j.id, error: "Sastra Compressor can't read this file's format.\nInvalid data found when processing input" }
                : {
                    state: 'done', id: j.id, outPath: j.path.replace(/\.\w+$/, '.out'), outSize: Math.round(samples[i % samples.length][2] * ratio[j.kind]), kept: false,
                    note: j.rel.endsWith('.gif') ? { text: 'Stays animated', warn: false } : j.rel.endsWith('.mov') ? { text: 'Transparency removed', warn: true } : null,
                  }));
          }, at + s * 120));
        }
        at += steps * 120 + 60;
      });
      timers.push(window.setTimeout(() => doneFns.forEach((f) => f(false)), at + 100));
    },
    cancel: async () => {
      cancelled = true;
      timers.forEach(clearTimeout); timers = [];
      setTimeout(() => doneFns.forEach((f) => f(cancelled)), 50);
    },
    existing: async (jobs, s) => (s.saveMode === 'folder' ? jobs.slice(0, 2).map((j) => j.path) : []),
    noBin: async (paths) => paths.filter((p) => p.includes('voice/')),
    version: async () => '2.1.1',
    onJob: async (fn) => { jobFns.push(fn); return () => jobFns.splice(jobFns.indexOf(fn), 1); },
    onFinished: async (fn) => { doneFns.push(fn); return () => doneFns.splice(doneFns.indexOf(fn), 1); },
    pickFiles: async () => ['C:/VN/assets'],
    pickFolders: async () => ['C:/VN/assets'],
    pickOutputFolder: async () => 'C:/VN/game/assets',
    reveal: async (p) => console.log('reveal', p),
    openUrl: async (u) => { window.open(u, '_blank'); },
    onDrop: async (fn) => { dropFns.push(fn); return () => dropFns.splice(dropFns.indexOf(fn), 1); },
    setTheme: async () => {},
    setProgress: async () => {},
    notify: async () => {},
    onCloseRequested: async () => () => {},
    quit: async () => {},
    minimize: async () => {},
    toggleMaximize: async () => {},
    close: async () => {},
    frameState: async () => null,
    onFrame: async () => () => {},
    setMaximizeButton: async () => {},
    systemMenu: async () => {},
    setBackground: async () => {},
    show: async () => {},
  };
}

export const backend: Promise<Backend> = inApp ? tauriBackend() : Promise.resolve(mockBackend());
