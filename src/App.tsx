import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { backend, type Job } from './lib/backend';
import type { Formats, Item, JobUpdate, Kind, Row, RunChoices, Settings } from './lib/types';
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
const folderOf = (path: string) => path.split(/[\\/]/).slice(0, -1).pop() ?? path;

/** One file's latest news, applied to its row. */
function applyUpdate(r: Row, u: JobUpdate, replacing: boolean): Row {
  if (u.state === 'running') return { ...r, status: { state: 'running', progress: u.progress } };
  if (u.state === 'done') return { ...r, status: { state: 'done', outPath: u.outPath, outSize: u.outSize, kept: u.kept, note: u.note, replaced: replacing && !u.kept } };
  if (u.state === 'failed') return { ...r, status: { state: 'failed', error: u.error } };
  return { ...r, status: { state: 'waiting' } };
}

export default function App() {
  const [rows, setRows] = useState<Row[]>([]);
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const [formats, setFormats] = useState<Formats | null>(null);
  const [phase, setPhase] = useState<Phase>('idle');
  const [outputs, setOutputs] = useState<Record<string, string>>({});
  const [over, setOver] = useState(false);
  const [about, setAbout] = useState(false);
  const [confirm, setConfirm] = useState<'quit' | 'replace' | 'no-bin' | 'existing' | null>(null);
  // How many originals can't go to the Bin, and which files would be overwritten: for the dialogs.
  const [noBin, setNoBin] = useState(0);
  const [clashes, setClashes] = useState<string[]>([]);
  // New files from a folder, some of them named like this app's output, awaiting an answer.
  const [found, setFound] = useState<Item[] | null>(null);
  const includeFound = useRef(false);
  const asking = useRef(false);
  const [stopping, setStopping] = useState(false);
  // The files in the current run: finished ones from an earlier run aren't this run's progress.
  const [runPaths, setRunPaths] = useState<Set<string>>(new Set());
  // Originals were deleted rather than binned, for the summary.
  const [deleted, setDeleted] = useState(false);
  // Files this session wrote; running again may overwrite those without asking.
  const written = useRef(new Set<string>());
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
  const addRows = useCallback((items: Item[]) => {
    if (!items.length) return;
    setRows((old) => {
      const now = new Set(old.map((x) => x.path));
      return [...old, ...items.filter((i) => !now.has(i.path)).map((i) => ({ ...i, status: { state: 'waiting' as const } }))];
    });
    setPhase((p) => (p === 'finished' ? 'idle' : p));
  }, []);

  const add = useCallback(async (paths: string[]) => {
    if (!paths.length) return;
    if (busyRef.current) { toast('Wait until compressing has finished, then add more', 'info'); return; }
    const r = await (await backend).scan(paths);
    const have = new Set(rowsRef.current.map((x) => x.path));
    const fresh = r.items.filter((i) => !have.has(i.path));
    if (fresh.some((i) => i.compressed)) {
      includeFound.current = false;
      // More may be dropped while the question is showing; it then covers them too.
      setFound((old) => {
        const held = new Set((old ?? []).map((i) => i.path));
        return [...(old ?? []), ...fresh.filter((i) => !held.has(i.path))];
      });
    } else {
      addRows(fresh);
    }
    // Say what was left out, so nothing goes missing silently.
    if (r.unsupported.length) {
      const shown = r.unsupported.slice(0, 2).join(', ');
      const more = r.unsupported.length > 2 ? ` and ${plural(r.unsupported.length - 2, 'other')}` : '';
      toast(`Skipped ${shown}${more}: not an image, audio, or video file this app can read`, 'warn');
    }
    if (!fresh.length && r.items.length) toast(r.items.length === 1 ? 'That file is already in the list' : 'Those files are already in the list', 'info');
    if (!r.items.length && !r.unsupported.length) toast('Nothing to add: the folder has no images, audio, or video', 'info');
  }, [addRows]);

  const pickFiles = async () => { if (formats) add(await (await backend).pickFiles(formats)); };
  const pickFolders = async () => add(await (await backend).pickFolders());
  const pickOutput = async () => {
    const f = await (await backend).pickOutputFolder();
    if (f) changeSettings({ ...settingsRef.current, saveMode: 'folder', outputFolder: f });
  };

  /* ---------- settings: a change makes finished files waiting again ---------- */
  const changeSettings = (s: Settings) => {
    settingsRef.current = s;
    setSettings(s);
    saveSettings(s);
    // A file whose original was replaced stays done: there is nothing left to compress again.
    setRows((old) => old.map((r) => (r.status.state === 'waiting' || (r.status.state === 'done' && r.status.replaced) ? r : { ...r, status: { state: 'waiting' } })));
    setPhase('idle');
  };

  /* ---------- where each file will go (the backend decides, so it matches) ---------- */
  // Not while compressing: nothing is planned then, and a long list gets a lot of updates.
  const planKey = useMemo(() => (busy ? '' : rows.filter(todo).map((r) => r.path).join('\n')), [rows, busy]);
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

  /* ---------- replacing: say up front which originals can't go to the Bin ---------- */
  const [noBinNow, setNoBinNow] = useState(0);
  useEffect(() => {
    if (busy || settings.saveMode !== 'replace') { setNoBinNow(0); return; }
    let stale = false;
    const paths = rowsRef.current.filter(todo).map((r) => r.path);
    backend.then((b) => b.noBin(paths)).then((l) => { if (!stale) setNoBinNow(l.length); }).catch(() => {});
    return () => { stale = true; };
  }, [planKey, settings.saveMode, busy]);

  /* ---------- compressing ---------- */
  const pendingCount = rows.filter(todo).length;
  // Everything that could lose something is asked about here, before any work starts.
  const askToStart = async () => {
    // These checks take a moment, and a second click mustn't start a second run.
    if (!pendingCount || busy || asking.current) return;
    asking.current = true;
    try {
      const b = await backend;
      const jobs = rowsRef.current.filter(todo).map(toJob);
      if (settings.saveMode === 'replace') {
        const without = await b.noBin(jobs.map((j) => j.path));
        setNoBin(without.length);
        setConfirm(without.length ? 'no-bin' : 'replace');
        return;
      }
      // "Alongside" names are this app's own, so only a chosen folder holds someone's other files.
      const there = settings.saveMode === 'folder' ? (await b.existing(jobs, settings)).filter((p) => !written.current.has(p)) : [];
      if (there.length) {
        setClashes(there);
        setConfirm('existing');
        return;
      }
      await start();
    } finally {
      asking.current = false;
    }
  };
  const start = async (choices: RunChoices = {}) => {
    const jobs = rowsRef.current.filter(todo).map(toJob);
    if (!jobs.length) return;
    setStopping(false);
    setRows((old) => old.map((r) => (r.status.state === 'failed' ? { ...r, status: { state: 'waiting' } } : r)));
    setRunPaths(new Set(jobs.map((j) => j.path)));
    setDeleted(!!choices.deleteForGood);
    setPhase('running');
    setStarted(Date.now());
    finishAt.current = 0;
    try {
      await (await backend).compress(jobs, { ...settingsRef.current, ...choices });
    } catch (e) {
      setPhase('idle');
      toast(String(e), 'err');
    }
  };
  const cancel = async () => {
    setStopping(true);
    await (await backend).cancel();
  };

  useEffect(() => {
    let disposed = false;
    const offs: (() => void)[] = [];
    const keep = (off: () => void) => (disposed ? off() : offs.push(off));
    // Many files report at once, and each update touches the whole list, so they're
    // gathered and applied together. (A timer: a minimised window gets no animation frames.)
    const pending = new Map<string, JobUpdate>();
    let timer = 0;
    const flush = () => {
      window.clearTimeout(timer);
      timer = 0;
      if (!pending.size) return;
      const batch = new Map(pending);
      pending.clear();
      const replacing = settingsRef.current.saveMode === 'replace';
      setRows((old) => old.map((r) => {
        const u = batch.get(r.path);
        return u ? applyUpdate(r, u, replacing) : r;
      }));
    };
    offs.push(() => window.clearTimeout(timer));
    backend.then(async (b) => {
      b.formats().then((f) => !disposed && setFormats(f));
      keep(await b.onJob((u: JobUpdate) => {
        if (u.state === 'done') written.current.add(u.outPath);
        pending.set(u.id, u);
        if (!timer) timer = window.setTimeout(flush, 100);
      }));
      keep(await b.onFinished((cancelled) => {
        flush();
        setStopping(false);
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
    const inRun = rows.filter((r) => (busy ? runPaths.has(r.path) : r.status.state !== 'waiting'));
    const all = inRun.reduce((n, r) => n + r.size, 0) || 1;
    let doneBytes = 0, finished = 0, failed = 0, before = 0, after = 0, bigger = 0, kept = 0, replaced = 0;
    for (const r of inRun) {
      const s = r.status;
      if (s.state === 'running') doneBytes += r.size * s.progress;
      if (s.state === 'done') {
        doneBytes += r.size; finished++; before += r.size; after += s.outSize;
        if (s.outSize > r.size) bigger++;
        if (s.kept) kept++;
        if (s.replaced) replaced++;
      }
      if (s.state === 'failed') { doneBytes += r.size; failed++; }
    }
    return { fraction: doneBytes / all, finished, failed, count: inRun.length, saved: before - after, before, bigger, kept, replaced };
  }, [rows, busy, runPaths]);

  // A run that ends while "Stop and quit?" is showing leaves nothing to stop.
  useEffect(() => {
    if (!busy) setConfirm((c) => (c === 'quit' ? null : c));
  }, [busy]);

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
          <SettingsPanel settings={settings} counts={counts} busy={busy} noBin={noBinNow} onChange={changeSettings} onPickFolder={pickOutput} />
        )}
      </main>

      {rows.length > 0 && (
      <footer className="action-bar">
        <div className="action-status" aria-live="polite">
          {phase === 'running' ? (
            <>
              <div className="status-line">
                <b>{stopping ? 'Stopping…' : `Compressing ${num(Math.min(run.finished + run.failed + 1, run.count))} of ${num(run.count)}`}</b>
                {!stopping && <span className="meta">{percent(run.fraction)}{eta && ` · ${eta}`}</span>}
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
              {run.kept > 0 && <span className="meta">{plural(run.kept, 'file')} already small, so kept as {run.kept === 1 ? 'it was' : 'they were'}</span>}
              {run.replaced > 0 && <span className="meta">{deleted ? 'Originals with no Recycle Bin were deleted; any others are in the Bin' : 'The originals are in the Recycle Bin'}</span>}
            </div>
          ) : (
            <span className="meta">{pendingCount === rows.length ? 'Ready when you are.' : pendingCount ? `${plural(pendingCount, 'file')} still to do.` : 'Everything in the list is done.'}</span>
          )}
        </div>
        <div className="btn-row">
          {phase === 'running' ? (
            <button type="button" className="btn btn-secondary" disabled={stopping} onClick={cancel}>Cancel</button>
          ) : phase === 'finished' && firstOut ? (
            <>
              <button type="button" className="btn btn-secondary" onClick={clear}>Clear list</button>
              {run.failed > 0 && (
                <button type="button" className="btn btn-secondary" onClick={askToStart}>
                  <i className="fa-solid fa-rotate-right" aria-hidden="true" />Try {run.failed === 1 ? 'it' : `${num(run.failed)}`} again
                </button>
              )}
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

      {confirm === 'no-bin' && (
        <Modal
          title={noBin === pendingCount
            ? `${noBin === 1 ? 'This original' : `These ${num(noBin)} originals`} can't go to the Recycle Bin`
            : `${num(noBin)} of ${plural(pendingCount, 'original')} can't go to the Recycle Bin`}
          icon="fa-triangle-exclamation"
          size="confirm"
          onClose={() => setConfirm(null)}
          foot={
            <>
              <button type="button" className="btn btn-secondary" onClick={(e) => closeModal(e.currentTarget)}>Cancel</button>
              <button type="button" className="btn btn-danger-solid" onClick={(e) => { closeModal(e.currentTarget); start({ deleteForGood: true }); }}>
                Replace and delete for good
              </button>
            </>
          }
        >
          <p>USB and network drives have no Recycle Bin, and very large files don't fit in one. Replacing deletes {noBin === 1 ? 'this original' : 'these originals'} for good, with no way to get {noBin === 1 ? 'it' : 'them'} back.</p>
          {noBin !== pendingCount && <p>The other originals go to the Recycle Bin as usual.</p>}
        </Modal>
      )}

      {confirm === 'existing' && (
        <Modal
          title={clashes.length === 1 ? 'A file with this name already exists' : `${num(clashes.length)} files with these names already exist`}
          icon="fa-copy"
          size="confirm"
          onClose={() => setConfirm(null)}
          foot={
            <>
              <button type="button" className="btn btn-secondary" onClick={(e) => closeModal(e.currentTarget)}>Cancel</button>
              <button type="button" className="btn btn-danger-solid" onClick={(e) => { closeModal(e.currentTarget); start(); }}>Overwrite</button>
              <button type="button" className="btn btn-primary" onClick={(e) => { closeModal(e.currentTarget); start({ keepExisting: true }); }}>Keep both</button>
            </>
          }
        >
          <p>
            In <b>{folderOf(clashes[0] ?? '')}</b>{clashes.length > 1 && ' and any subfolders'}. <b>Keep both</b> gives the new {clashes.length === 1 ? 'file a number' : 'files numbers'}, like <b>name_2</b>. <b>Overwrite</b> replaces the existing {clashes.length === 1 ? 'file' : 'files'} for good; {clashes.length === 1 ? 'it doesn\'t' : 'they don\'t'} go to the Recycle Bin.
          </p>
        </Modal>
      )}

      {found && (
        <Modal
          title={(() => { const n = found.filter((i) => i.compressed).length; return n === 1 ? 'Found a file this app already compressed' : `Found ${num(n)} files this app already compressed`; })()}
          icon="fa-compress"
          size="confirm"
          onClose={() => {
            addRows(includeFound.current ? found : found.filter((i) => !i.compressed));
            setFound(null);
          }}
          foot={
            <>
              <button type="button" className="btn btn-secondary" onClick={(e) => { includeFound.current = true; closeModal(e.currentTarget); }}>Include</button>
              <button type="button" className="btn btn-primary" onClick={(e) => closeModal(e.currentTarget)}>Leave out</button>
            </>
          }
        >
          <p>Their names end in <b>_compressed</b>. Compressing them again usually lowers the quality without saving much space.</p>
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
              <button type="button" className="btn btn-danger-solid" onClick={async () => (await backend).quit()}>
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
