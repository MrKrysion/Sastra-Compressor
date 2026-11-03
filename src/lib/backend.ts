// Everything the interface asks of the app. Inside Tauri it talks to the
// Rust side; in a plain browser (`npm run dev`) a stand-in fakes the files
// and the compressing, so the design can be worked on without building.
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { Formats, JobUpdate, Kind, ScanResult, Settings } from './types';

export const inApp = isTauri();

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
  compress(jobs: Job[], settings: Settings): Promise<void>;
  cancel(): Promise<void>;
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
    quit: () => win.destroy(),
    show: () => win.show(),
  };
}

/* ---------- the browser stand-in ---------- */
function mockBackend(): Backend {
  const formats: Formats = {
    image: ['png', 'jpg', 'jpeg', 'webp', 'bmp', 'tif', 'tiff', 'gif'],
    audio: ['wav', 'flac', 'mp3', 'ogg', 'oga', 'opus', 'm4a', 'aac', 'aif', 'aiff', 'w64'],
    video: ['mp4', 'm4v', 'mov', 'mkv', 'webm', 'avi'],
  };
  const samples: [string, Kind, number][] = [
    ['bg/school_hallway.png', 'image', 6_811_000],
    ['bg/rooftop_sunset.png', 'image', 7_402_000],
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
          path: `${p}/${rel}`, name: rel.split('/').pop()!, rel, kind, size,
        })),
      ),
      unsupported: ['notes.txt'],
    }),
    plan: async (jobs, s) => {
      const ext = { image: 'webp', audio: 'ogg', video: 'mp4' } as const;
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
                : { state: 'done', id: j.id, outPath: j.path.replace(/\.\w+$/, '.out'), outSize: Math.round(samples[i % samples.length][2] * ratio[j.kind]) }));
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
    version: async () => '2.0.0',
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
    show: async () => {},
  };
}

export const backend: Promise<Backend> = inApp ? tauriBackend() : Promise.resolve(mockBackend());
