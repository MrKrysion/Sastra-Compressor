import { useState, type ReactNode } from 'react';
import type { Kind, Level, SaveMode, Settings } from '../lib/types';
import { Seg } from './Seg';
import { num, plural } from '../lib/text';

const QUALITY: { value: Level; label: string; tip: string }[] = [
  { value: 'smaller', label: 'Smaller', tip: 'Smallest files; some detail is lost' },
  { value: 'balanced', label: 'Balanced', tip: 'Small files that still look sharp' },
  { value: 'sharper', label: 'Sharper', tip: 'Close to the original; larger files' },
];
const WIDTH = [
  { value: 0, label: 'Original' },
  { value: 1920, label: '1920 px' },
  { value: 1280, label: '1280 px' },
];
const FRAME_RATE = [
  { value: 0, label: 'Original' },
  { value: 60, label: '60 fps', tip: 'Faster videos are slowed to 60; slower ones stay as they are' },
  { value: 30, label: '30 fps', tip: 'Faster videos are slowed to 30; slower ones stay as they are' },
];
const SAVE: { value: SaveMode; label: string }[] = [
  { value: 'beside', label: 'Alongside' },
  { value: 'replace', label: 'Replace' },
  { value: 'folder', label: 'Folder' },
];
const KINDS: Kind[] = ['image', 'audio', 'video'];

const qualityName = (l: Level) => QUALITY.find((q) => q.value === l)!.label;

/** The choices at a glance, for a closed row; anything left at "Original" goes unsaid. */
function summary(kind: Kind, s: Settings): string {
  if (kind === 'audio') return s.audioBitrate === 96 ? 'Music' : 'Voice and sounds';
  const parts = kind === 'image'
    ? [qualityName(s.imageQuality), s.imageMaxWidth && `${s.imageMaxWidth} px`]
    : [qualityName(s.videoQuality), s.videoMaxWidth && `${s.videoMaxWidth} px`, s.videoMaxFps && `${s.videoMaxFps} fps`];
  return parts.filter(Boolean).join(' · ');
}

interface Props {
  settings: Settings;
  counts: Record<Kind, number>;
  busy: boolean;
  /** How many originals can't go to the Recycle Bin if replaced. */
  noBin: number;
  onChange(s: Settings): void;
  onPickFolder(): void;
}

/**
 * "Save to" is always open. Below it, one collapsible row per kind of file in the list
 * at most one open at a time so the panel rarely needs
 * to scroll. A closed row still shows its choices, so nothing is hidden.
 */
export function SettingsPanel({ settings: s, counts, busy, noBin, onChange, onPickFolder }: Props) {
  const set = (patch: Partial<Settings>) => onChange({ ...s, ...patch });
  const present = KINDS.filter((k) => counts[k] > 0);
  const [chosen, setChosen] = useState<Kind | 'none' | null>(null);
  // The row last opened, or none if it was closed; if its files have left the list, the first that's still there.
  const open = chosen === 'none' ? null : chosen && present.includes(chosen) ? chosen : present[0];
  const folderName = s.outputFolder?.split(/[\\/]/).filter(Boolean).pop() ?? '';

  const group = (kind: Kind, title: string, icon: string, fields: ReactNode) => {
    if (!counts[kind]) return null;
    const isOpen = open === kind;
    const label = <><i className={`fa-solid ${icon}`} aria-hidden="true" />{title}</>;
    // With only one kind in the list there's nothing to collapse.
    if (present.length === 1) {
      return (
        <section className="card">
          <div className="set-head">
            <h2>{label}</h2>
            <span className="meta">{plural(counts[kind], 'file')}</span>
          </div>
          {fields}
        </section>
      );
    }
    return (
      <section className={`card acc-item set-acc${isOpen ? ' open' : ''}`}>
        <button type="button" className="acc-q" aria-expanded={isOpen} onClick={() => setChosen(isOpen ? 'none' : kind)}>
          <h2>{label}</h2>
          <small className="meta">{isOpen ? plural(counts[kind], 'file') : summary(kind, s)}</small>
          <i className="fa-solid fa-chevron-down" aria-hidden="true" />
        </button>
        <div className="acc-a" inert={!isOpen}>
          <div><div className="set-fields">{fields}</div></div>
        </div>
      </section>
    );
  };

  return (
    <aside className="settings scroll-thin" aria-label="Settings">
      <section className="card">
        <div className="set-head">
          <h2><i className="fa-solid fa-folder" aria-hidden="true" />Save to</h2>
        </div>
        <Seg
          label="Where to save"
          value={s.saveMode}
          disabled={busy}
          options={SAVE}
          onChange={(v) => (v === 'folder' && !s.outputFolder ? onPickFolder() : set({ saveMode: v }))}
        />
        {s.saveMode === 'beside' && (
          <p className="hint">Saved next to each original as <b>name_compressed</b>. The originals stay as they are.</p>
        )}
        {s.saveMode === 'replace' && (
          <p className="hint">Each original is swapped for its compressed version, with the same name and the new file type. The originals go to the Recycle Bin.</p>
        )}
        {s.saveMode === 'replace' && noBin > 0 && (
          <p className="hint warn-text">
            <i className="fa-solid fa-triangle-exclamation" aria-hidden="true" />
            <span>{noBin === 1 ? 'One original' : `${num(noBin)} originals`} can't go to the Recycle Bin (on a USB drive, for example), so {noBin === 1 ? 'it is' : 'they are'} deleted for good.</span>
          </p>
        )}
        {s.saveMode === 'folder' && s.outputFolder && (
          <>
            <button type="button" className="folder-pick" disabled={busy} onClick={onPickFolder} data-tip={s.outputFolder} data-tip-always="">
              <i className="fa-solid fa-folder-open" aria-hidden="true" />
              <span>{folderName}</span>
              <small>Change</small>
            </button>
            <p className="hint">Same names and subfolders as the originals, ready to drop into your Sastra™ project.</p>
          </>
        )}
      </section>

      {group('image', 'Images', 'fa-image', (
        <>
          <span className="field-label">Quality</span>
          <Seg label="Image quality" value={s.imageQuality} options={QUALITY} disabled={busy} onChange={(v) => set({ imageQuality: v })} />
          <span className="field-label">Maximum width</span>
          <Seg label="Maximum image width" value={s.imageMaxWidth} options={WIDTH} disabled={busy} onChange={(v) => set({ imageMaxWidth: v })} />
        </>
      ))}

      {group('audio', 'Audio', 'fa-music', (
        <>
          <Seg
            label="Audio type"
            value={s.audioBitrate}
            disabled={busy}
            options={[
              { value: 96, label: 'Music' },
              { value: 64, label: 'Voice and sounds' },
            ]}
            onChange={(v) => set({ audioBitrate: v })}
          />
          <p className="hint">
            {s.audioBitrate === 96
              ? <>Opus at <b>96 kbit/s</b>: clear music at a fraction of the size.</>
              : <>Opus at <b>64 kbit/s</b>: plenty for speech and sound effects.</>}
          </p>
        </>
      ))}

      {group('video', 'Video', 'fa-film', (
        <>
          <span className="field-label">Quality</span>
          <Seg label="Video quality" value={s.videoQuality} options={QUALITY} disabled={busy} onChange={(v) => set({ videoQuality: v })} />
          <span className="field-label">Maximum width</span>
          <Seg label="Maximum video width" value={s.videoMaxWidth} options={WIDTH} disabled={busy} onChange={(v) => set({ videoMaxWidth: v })} />
          <span className="field-label">Maximum frame rate</span>
          <Seg label="Maximum frame rate" value={s.videoMaxFps} options={FRAME_RATE} disabled={busy} onChange={(v) => set({ videoMaxFps: v })} />
        </>
      ))}
    </aside>
  );
}
