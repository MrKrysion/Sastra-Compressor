import type { Kind, Level, SaveMode, Settings } from '../lib/types';
import { Seg } from './Seg';
import { plural } from '../lib/text';

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
const SAVE: { value: SaveMode; label: string }[] = [
  { value: 'beside', label: 'Alongside' },
  { value: 'replace', label: 'Replace' },
  { value: 'folder', label: 'Folder' },
];

interface Props {
  settings: Settings;
  counts: Record<Kind, number>;
  busy: boolean;
  onChange(s: Settings): void;
  onPickFolder(): void;
}

/** One card per setting group; a kind with no files in the list shows no card. */
export function SettingsPanel({ settings: s, counts, busy, onChange, onPickFolder }: Props) {
  const set = (patch: Partial<Settings>) => onChange({ ...s, ...patch });
  const head = (title: string, icon: string, kind?: Kind) => (
    <div className="set-head">
      <h2><i className={`fa-solid ${icon}`} aria-hidden="true" />{title}</h2>
      {kind && <span className="meta">{plural(counts[kind], 'file')}</span>}
    </div>
  );
  const folderName = s.outputFolder?.split(/[\\/]/).filter(Boolean).pop() ?? '';

  return (
    <aside className="settings scroll-thin" aria-label="Settings">
      <section className="card">
        {head('Save to', 'fa-folder')}
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

      {counts.image > 0 && (
        <section className="card">
          {head('Images', 'fa-image', 'image')}
          <span className="field-label">Quality</span>
          <Seg label="Image quality" value={s.imageQuality} options={QUALITY} disabled={busy} onChange={(v) => set({ imageQuality: v })} />
          <span className="field-label">Maximum width</span>
          <Seg label="Maximum image width" value={s.imageMaxWidth} options={WIDTH} disabled={busy} onChange={(v) => set({ imageMaxWidth: v })} />
        </section>
      )}

      {counts.audio > 0 && (
        <section className="card">
          {head('Audio', 'fa-music', 'audio')}
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
        </section>
      )}

      {counts.video > 0 && (
        <section className="card">
          {head('Video', 'fa-film', 'video')}
          <span className="field-label">Quality</span>
          <Seg label="Video quality" value={s.videoQuality} options={QUALITY} disabled={busy} onChange={(v) => set({ videoQuality: v })} />
          <span className="field-label">Maximum width</span>
          <Seg label="Maximum video width" value={s.videoMaxWidth} options={WIDTH} disabled={busy} onChange={(v) => set({ videoMaxWidth: v })} />
          <p className="hint">The sound in videos becomes AAC at 128 kbit/s.</p>
        </section>
      )}
    </aside>
  );
}
