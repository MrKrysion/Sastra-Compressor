import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { backend, type Job } from './lib/backend';
import type { Formats, JobUpdate, Kind, Row, Settings } from './lib/types';
import { loadSettings, saveSettings } from './lib/settings';
import { bytes, list, num, percent, plural, timeLeft } from './lib/text';
import { Header } from './components/Header';
import { FileList } from './components/FileList';
import { SettingsPanel } from './components/SettingsPanel';
import { About } from './components/About';
import { Modal, closeModal } from './components/Modal';

type Phase = 'idle' | 'running' | 'finished';
const toast = (m: string, k?: 'info' | 'ok' | 'err' | 'warn') => window.Krysion.toast(m, k);
const NAMES: Record<Kind, string> = { image: 'images', audio: 'audio', video: 'video' };
const toJob = (r: Row): Job => ({ id: r.path, path: r.path, rel: r.rel, kind: r.kind });
const todo = (r: Row) => r.status.state === 'waiting' || r.status.state === 'failed';

export default function App() {
  const [rows, setRows] = useState<Row[]>([]);
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const [formats, setFormats] = useState<Formats | null>(null);
  const [phase, setPhase] = useState<Phase>('idle');
  const [outputs, setOutputs] = useState<Record<string, string>>({});
  const [over, setOver] = useState(false);
  const [about, setAbout] = useState(false);
  const [confirm, setConfirm] = useState<'quit' | 'replace' | null>(null);
  const [started, setStarted] = useState(0);
  const [now, setNow] = useState(Date.now());
  // The estimated finish time, smoothed so the countdown doesn't jump about.
  const finishAt = useRef(0);
  const busy = phase === 'running';

  // The latest values, for listeners that live as long as the window.
  const rowsRef = useRef(rows);
  rowsRef.current = rows;
  const busyRef = useRef(busy);
  busyRef.current = busy;
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  /* ---------- adding files ---------- */
  const add = useCallback(async (paths: string[]) => {
    if (!paths.length) return;
    if (busyRef.current) { toast('Wait until compressing has finished, then add more', 'info'); return; }
    const r = await (await backend).scan(paths);
    const have = new Set(rowsRef.current.map((x) => x.path));
    const fresh = r.items.filter((i) => !have.has(i.path));
    if (fresh.length) {
      setRows((old) => {
        const now = new Set(old.map((x) => x.path));
        return [...old, ...fresh.filter((i) => !now.has(i.path)).map((i) => ({ ...i, status: { state: 'waiting' as const } }))];
      });
      setPhase((p) => (p === 'finished' ? 'idle' : p));
    }
    // Say what was left out, so nothing goes missing silently.
    if (r.unsupported.length) {
      const shown = r.unsupported.slice(0, 2).join(', ');
      const more = r.unsupported.length > 2 ? ` and ${plural(r.unsupported.length - 2, 'other')}` : '';
      toast(`Skipped ${shown}${more}: not an image, audio, or video file this app can read`, 'warn');
    }
    if (!fresh.length && r.items.length) toast(r.items.length === 1 ? 'That file is already in the list' : 'Those files are already in the list', 'info');
    if (!r.items.length && !r.unsupported.length) toast('Nothing to add: the folder has no images, audio, or video', 'info');
  }, []);

  const pickFiles = async () => { if (formats) add(await (await backend).pickFiles(formats)); };
  const pickFolders = async () => add(await (await backend).pickFolders());
  const pickOutput = async () => {
    const f = await (await backend).pickOutputFolder();
    if (f) changeSettings({ ...settingsRef.current, saveMode: 'folder', outputFolder: f });
  };

  /* ---------- settings: a change makes finished files waiting again ---------- */
  const changeSettings = (s: Settings) => {
    setSettings(s);
    saveSettings(s);
    // A file whose original was replaced stays done: there is nothing left to compress again.
    setRows((old) => old.map((r) => (r.status.state === 'waiting' || (r.status.state === 'done' && r.status.replaced) ? r : { ...r, status: { state: 'waiting' } })));
    setPhase('idle');
  };

  /* ---------- where each file will go (the backend decides, so it matches) ---------- */
  const planKey = rows.filter(todo).map((r) => r.path).join('\n');
  useEffect(() => {
    if (busy) return;
    let stale = false;
    const pending = rowsRef.current.filter(todo);
    backend.then((b) => b.plan(pending.map(toJob), settings)).then((out) => {
      if (stale) return;
      const map: Record<string, string> = {};
      pending.forEach((r, i) => { map[r.path] = out[i]; });
      setOutputs(map);
    }).catch(() => {});
    return () => { stale = true; };
  }, [planKey, settings, busy]);

  /* ---------- compressing ---------- */
  const pendingCount = rows.filter(todo).length;
  const askToStart = () => {
    if (!pendingCount || busy) return;
    if (settings.saveMode === 'replace') setConfirm('replace');
    else start();
  };
  const start = async () => {
    const jobs = rowsRef.current.filter(todo).map(toJob);
    if (!jobs.length) return;
    setRows((old) => old.map((r) => (r.status.state === 'failed' ? { ...r, status: { state: 'waiting' } } : r)));
    setPhase('running');
    setStarted(Date.now());
    finishAt.current = 0;
    try {
      await (await backend).compress(jobs, settingsRef.current);
    } catch (e) {
      setPhase('idle');
      toast(String(e), 'err');
    }
  };
  const cancel = async () => (await backend).cancel();

  useEffect(() => {
    let disposed = false;
    const offs: (() => void)[] = [];
    const keep = (off: () => void) => (disposed ? off() : offs.push(off));
    backend.then(async (b) => {
      b.formats().then((f) => !disposed && setFormats(f));
      keep(await b.onJob((u: JobUpdate) => {
        setRows((old) => old.map((r) => {
          if (r.path !== u.id) return r;
          if (u.state === 'running') return { ...r, status: { state: 'running', progress: u.progress } };
          if (u.state === 'done') return { ...r, status: { state: 'done', outPath: u.outPath, outSize: u.outSize, replaced: settingsRef.current.saveMode === 'replace' } };
          if (u.state === 'failed') return { ...r, status: { state: 'failed', error: u.error } };
          return { ...r, status: { state: 'waiting' } };
        }));
      }));
      keep(await b.onFinished((cancelled) => {
        setPhase(cancelled ? 'idle' : 'finished');
        b.setProgress(null);
        if (cancelled) toast('Stopped. Files already done were kept', 'info');
      }));
      keep(await b.onDrop((e) => {
        setOver(e.type === 'over' && !busyRef.current);
        if (e.type === 'drop') add(e.paths);
      }));
      keep(await b.onCloseRequested(() => {
        if (!busyRef.current) return true;
        setConfirm('quit');
        return false;
      }));
    });
    return () => { disposed = true; offs.forEach((f) => f()); };
  }, [add]);

  /* ---------- totals ---------- */
  const total = useMemo(() => rows.reduce((n, r) => n + r.size, 0), [rows]);
  const counts = useMemo(() => {
    const c: Record<Kind, number> = { image: 0, audio: 0, video: 0 };
    rows.forEach((r) => c[r.kind]++);
    return c;
  }, [rows]);
  const run = useMemo(() => {
    // Progress weighted by size: big files take longer.
    const inRun = rows.filter((r) => r.status.state !== 'waiting' || busy);
    const all = inRun.reduce((n, r) => n + r.size, 0) || 1;
    let doneBytes = 0, finished = 0, failed = 0, before = 0, after = 0, bigger = 0;
    for (const r of inRun) {
      const s = r.status;
      if (s.state === 'running') doneBytes += r.size * s.progress;
      if (s.state === 'done') { doneBytes += r.size; finished++; before += r.size; after += s.outSize; if (s.outSize > r.size) bigger++; }
      if (s.state === 'failed') { doneBytes += r.size; failed++; }
    }
    return { fraction: doneBytes / all, finished, failed, count: inRun.length, saved: before - after, before, bigger };
  }, [rows, busy]);

  useEffect(() => {
    if (busy) backend.then((b) => b.setProgress(run.fraction));
  }, [busy, run.fraction]);

  useEffect(() => {
    if (phase !== 'finished') return;
    // The header already says "Sastra™ Compressor", so the title says what happened.
    const title = run.failed ? 'Finished, with problems' : 'All done!';
    const body = run.failed
      ? `${plural(run.finished, 'file')} compressed; ${num(run.failed)} couldn't be.`
      : `${plural(run.finished, 'file')} compressed, ${bytes(Math.max(0, run.saved))} saved.`;
    backend.then((b) => b.notify(title, body)).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [phase]);

  /* ---------- keyboard: Ctrl+O files, Ctrl+Shift+O a folder, Ctrl+Enter compress ---------- */
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!e.ctrlKey || busyRef.current || document.querySelector('.modal')) return;
      if (e.key.toLowerCase() === 'o') { e.preventDefault(); if (e.shiftKey) pickFolders(); else pickFiles(); }
      if (e.key === 'Enter') { e.preventDefault(); askToStart(); }
    };
    document.addEventListener('keydown', key);
    return () => document.removeEventListener('keydown', key);
  });

  // Tick every second while compressing, so the time left counts down between progress reports.
  useEffect(() => {
    if (!busy) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [busy]);
  // Each progress report gives a new guess at the finish time; blend it with the last one.
  useEffect(() => {
    if (!busy || run.fraction < 0.01) return;
    const elapsed = Date.now() - started;
    if (elapsed < 2000) return;
    const guess = started + elapsed / run.fraction;
    finishAt.current = finishAt.current ? finishAt.current * 0.8 + guess * 0.2 : guess;
  }, [busy, run.fraction, started]);
  const eta = busy && finishAt.current ? timeLeft((finishAt.current - now) / 1000) : '';
  const firstOut = rows.map((r) => r.status).find((s): s is Extract<Row['status'], { state: 'done' }> => s.state === 'done');
  const kindsAdded = (Object.keys(counts) as Kind[]).filter((k) => counts[k]).map((k) => NAMES[k]);
  const clear = () => { setRows([]); setPhase('idle'); };

  return (
    <div className="app">
      <Header onAbout={() => setAbout(true)} />

      <main className={`app-main${rows.length ? '' : ' no-files'}`}>
        <section className={`list-card card drop-target${over ? ' over' : ''}`} aria-label="Files">
          {rows.length === 0 ? (
            <div className={`drop-panel big${over ? ' over' : ''}`}>
              <i className="fa-solid fa-upload drop-icon" aria-hidden="true" />
              <h2>Drop images, audio, and video here</h2>
              <p>Whole folders work too: everything inside is added, subfolders included.</p>
              <div className="btn-row">
                <button type="button" className="btn btn-primary" onClick={pickFiles}>
                  <i className="fa-solid fa-plus" aria-hidden="true" />Add files
                </button>
                <button type="button" className="btn btn-secondary" onClick={pickFolders}>
                  <i className="fa-solid fa-folder-plus" aria-hidden="true" />Add a folder
                </button>
              </div>
              {formats && (
                <small>
                  {list(formats.image.filter((x) => !['jpeg', 'tif'].includes(x)).map(up))} images<br />
                  {list(formats.audio.filter((x) => !['oga', 'aif'].includes(x)).map(up))} audio<br />
                  {list(formats.video.filter((x) => x !== 'm4v').map(up))} video
                </small>
              )}
            </div>
          ) : (
            <>
              <div className="list-head">
                <div className="list-title">
                  <h2>{plural(rows.length, 'file')}</h2>
                  <span className="meta">{bytes(total)} · {list(kindsAdded)}</span>
                </div>
                <div className="btn-row">
                  <button type="button" className="btn btn-secondary btn-sm" disabled={busy} onClick={pickFiles}>
                    <i className="fa-solid fa-plus" aria-hidden="true" />Add files
                  </button>
                  <button type="button" className="btn btn-secondary btn-sm" disabled={busy} onClick={pickFolders}>
                    <i className="fa-solid fa-folder-plus" aria-hidden="true" />Add a folder
                  </button>
                  <button type="button" className="btn btn-text btn-sm" disabled={busy} onClick={clear}>
                    Clear list
                  </button>
                </div>
              </div>
              <div className="list-scroll scroll-thin">
                <FileList
                  rows={rows}
                  outputs={outputs}
                  busy={busy}
                  onRemove={(p) => setRows((old) => old.filter((r) => r.path !== p))}
                  onReveal={(p) => backend.then((b) => b.reveal(p)).catch(() => toast("Couldn't open the folder", 'err'))}
                />
              </div>
            </>
          )}
          <div className="drop-cover" aria-hidden="true">
            <i className="fa-solid fa-upload" />
            Drop to add
          </div>
        </section>

        {rows.length > 0 && (
          <SettingsPanel settings={settings} counts={counts} busy={busy} onChange={changeSettings} onPickFolder={pickOutput} />
        )}
      </main>

      {rows.length > 0 && (
      <footer className="action-bar">
        <div className="action-status" aria-live="polite">
          {phase === 'running' ? (
            <>
              <div className="status-line">
                <b>Compressing {num(Math.min(run.finished + run.failed + 1, run.count))} of {num(run.count)}</b>
                <span className="meta">{percent(run.fraction)}{eta && ` · ${eta}`}</span>
              </div>
              <span className="progress"><span style={{ width: `${Math.max(1, run.fraction * 100)}%` }} /></span>
            </>
          ) : phase === 'finished' ? (
            <div className="status-line">
              {run.failed ? (
                <span className="warn-text"><i className="fa-solid fa-triangle-exclamation" aria-hidden="true" />
                  <b>{plural(run.finished, 'file')} done; {num(run.failed)} couldn't be compressed</b>
                </span>
              ) : (
                <span className="ok-text"><i className="fa-solid fa-circle-check" aria-hidden="true" />
                  <b>{run.saved > 0 ? `All done! ${bytes(run.saved)} saved` : 'All done!'}</b>
                </span>
              )}
              {run.before > 0 && run.saved > 0 && <span className="meta">{percent(run.saved / run.before)} smaller overall</span>}
              {run.bigger > 0 && <span className="meta">{plural(run.bigger, 'file')} came out bigger than the original</span>}
              {settings.saveMode === 'replace' && run.finished > 0 && <span className="meta">The originals are in the Recycle Bin</span>}
            </div>
          ) : (
            <span className="meta">{pendingCount === rows.length ? 'Ready when you are.' : pendingCount ? `${plural(pendingCount, 'file')} still to do.` : 'Everything in the list is done.'}</span>
          )}
        </div>
        <div className="btn-row">
          {phase === 'running' ? (
            <button type="button" className="btn btn-secondary" onClick={cancel}>Cancel</button>
          ) : phase === 'finished' && firstOut ? (
            <>
              <button type="button" className="btn btn-secondary" onClick={clear}>Clear list</button>
              <button type="button" className="btn btn-primary" onClick={() => backend.then((b) => b.reveal(firstOut.outPath)).catch(() => toast("Couldn't open the folder", 'err'))}>
                <i className="fa-solid fa-folder-open" aria-hidden="true" />Show files
              </button>
            </>
          ) : (
            <button type="button" className="btn btn-primary" disabled={!pendingCount} onClick={askToStart}>
              <i className="fa-solid fa-compress" aria-hidden="true" />
              {pendingCount ? `Compress ${plural(pendingCount, 'file')}` : 'Compress'}
            </button>
          )}
        </div>
      </footer>
      )}

      {about && <About onClose={() => setAbout(false)} />}

      {confirm === 'replace' && (
        <Modal
          title={`Replace ${plural(pendingCount, 'original')}?`}
          icon="fa-arrow-right-arrow-left"
          size="confirm"
          onClose={() => setConfirm(null)}
          foot={
            <>
              <button type="button" className="btn btn-secondary" onClick={(e) => closeModal(e.currentTarget)}>Cancel</button>
              <button type="button" className="btn btn-primary" onClick={(e) => { closeModal(e.currentTarget); start(); }}>
                <i className="fa-solid fa-arrow-right-arrow-left" aria-hidden="true" />Replace
              </button>
            </>
          }
        >
          <p>Each original is swapped for its compressed version and moved to the Recycle Bin, so you can still get it back from there.</p>
        </Modal>
      )}

      {confirm === 'quit' && (
        <Modal
          title="Stop compressing and quit?"
          icon="fa-circle-stop"
          size="confirm"
          onClose={() => setConfirm(null)}
          foot={
            <>
              <button type="button" className="btn btn-secondary" onClick={(e) => closeModal(e.currentTarget)}>Keep going</button>
              <button type="button" className="btn btn-danger-solid" onClick={async () => { const b = await backend; await b.cancel(); setTimeout(() => b.quit(), 300); }}>
                <i className="fa-solid fa-xmark" aria-hidden="true" />Stop and quit
              </button>
            </>
          }
        >
          <p>The file being compressed now is left unfinished. Files already done are kept.</p>
        </Modal>
      )}
    </div>
  );
}

const up = (s: string) => ({ webp: 'WebP', webm: 'WebM', opus: 'Opus' } as Record<string, string>)[s] ?? s.toUpperCase();
