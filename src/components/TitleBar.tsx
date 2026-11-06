// The custom title bar. It is the app's header, so it holds the app's name, its own
// buttons, and the window controls. It knows nothing about this particular app; the
// contents come from its props. The native side is src-tauri/src/frame.rs.
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { backend, inApp, type FrameState } from '../lib/backend';

interface Props {
  /** The name or logo, and menus. */
  start: ReactNode;
  /** Optional, centred: a project's name, say. */
  center?: ReactNode;
  /** The app's own buttons, before the window controls. */
  end?: ReactNode;
  /** Blocks `start`, `center`, and `end` while a dialog is open; the window controls keep working. */
  inert?: boolean;
}

const RESTING: FrameState = { maximized: false, active: true, hover: false, pressed: false };

export function TitleBar({ start, center, end, inert }: Props) {
  const [frame, setFrame] = useState(RESTING);
  const maximize = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    let disposed = false;
    let off = () => {};
    backend.then(async (b) => {
      const stop = await b.onFrame(setFrame);
      if (disposed) stop(); else off = stop;
      const now = await b.frameState();
      if (now && !disposed) setFrame(now);
    }).catch(() => {});
    return () => { disposed = true; off(); };
  }, []);

  // Tells the native side where the button is, whenever that changes.
  useEffect(() => {
    const el = maximize.current;
    if (!el) return;
    const report = () => {
      const r = el.getBoundingClientRect();
      backend.then((b) => b.setMaximizeButton(innerWidth - r.right, r.width, r.height)).catch(() => {});
    };
    report();
    const watch = new ResizeObserver(report);
    watch.observe(el);
    watch.observe(document.documentElement);
    return () => watch.disconnect();
  }, []);

  // Right-clicking the bar or its window controls opens the system menu, as on any window.
  const menu = (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest('.tb-app')) return;
    e.preventDefault();
    backend.then((b) => b.systemMenu(false)).catch(() => {});
  };

  // Alt+Space has to be caught here too: the webview holds the keyboard focus.
  useEffect(() => {
    if (!inApp) return;
    const key = (e: KeyboardEvent) => {
      if (e.altKey && e.code === 'Space') {
        e.preventDefault();
        backend.then((b) => b.systemMenu(true)).catch(() => {});
      }
    };
    document.addEventListener('keydown', key);
    return () => document.removeEventListener('keydown', key);
  }, []);

  const act = (fn: 'minimize' | 'toggleMaximize' | 'close') => () => backend.then((b) => b[fn]()).catch(() => {});
  const drag = inApp ? { 'data-tauri-drag-region': 'deep' } : {};

  return (
    <header className={`titlebar${frame.active ? '' : ' inactive'}`} onContextMenu={menu} {...drag}>
      <div className="tb-start" inert={inert}>{start}</div>
      {center && <div className="tb-center" inert={inert}>{center}</div>}
      <div className="tb-end">
        {end && <div className="tb-app" inert={inert} data-tauri-drag-region="false">{end}</div>}
        <div className="tb-window">
          <button type="button" className="tb-btn" aria-label="Minimise" onClick={act('minimize')}>
            <svg viewBox="0 0 14 14" aria-hidden="true"><path d="M2 7h10" /></svg>
          </button>
          <button
            ref={maximize}
            type="button"
            className={`tb-btn${frame.hover ? ' hover' : ''}${frame.pressed ? ' pressed' : ''}`}
            aria-label={frame.maximized ? 'Restore' : 'Maximise'}
            onClick={act('toggleMaximize')}
          >
            {frame.maximized ? (
              <svg viewBox="0 0 14 14" aria-hidden="true"><rect x="1.8" y="4.2" width="8" height="8" rx="2.2" /><path d="M4.6 1.8h5.2a2.4 2.4 0 0 1 2.4 2.4v5.2" /></svg>
            ) : (
              <svg viewBox="0 0 14 14" aria-hidden="true"><rect x="2" y="2" width="10" height="10" rx="2.5" /></svg>
            )}
          </button>
          <button type="button" className="tb-btn tb-close" aria-label="Close" onClick={act('close')}>
            <svg viewBox="0 0 14 14" aria-hidden="true"><path d="M3 3l8 8M11 3l-8 8" /></svg>
          </button>
        </div>
      </div>
    </header>
  );
}
