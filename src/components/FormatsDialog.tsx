// "Every format": what this computer reads, by kind, with a search that answers
// "will my file work?" for one extension or format name.
import { useState } from 'react';
import { Modal } from './Modal';
import { Seg } from './Seg';
import type { Formats, Kind } from '../lib/types';

export const KINDS: { kind: Kind; title: string; icon: string; becomes: string }[] = [
  { kind: 'image', title: 'Pictures', icon: 'fa-image', becomes: 'WebP' },
  { kind: 'audio', title: 'Sound', icon: 'fa-music', becomes: 'Opus' },
  { kind: 'video', title: 'Video', icon: 'fa-film', becomes: 'WebM' },
];

export function FormatsDialog({ formats, start, onClose }: { formats: Formats; start: Kind | 'all'; onClose(): void }) {
  const [tab, setTab] = useState<Kind | 'all'>(start);
  const [query, setQuery] = useState('');
  const q = query.trim().toLowerCase().replace(/^\./, '');
  const count = formats.list.reduce((n, f) => n + f.ext.length, 0);
  const matches = (name: string, ext: string[]) => !q || name.toLowerCase().includes(q) || ext.some((e) => e.startsWith(q));
  // A search looks through every kind: ".mkv isn't supported" on the Sound tab would be wrong.
  const shown = KINDS.filter((k) => q || tab === 'all' || tab === k.kind)
    .map((k) => ({ ...k, rows: formats.list.filter((f) => f.kind === k.kind && matches(f.name, f.ext)) }))
    .filter((k) => k.rows.length);

  return (
    <Modal
      title="Every format"
      icon="fa-file-circle-check"
      sub={`${count} file types, read on this computer`}
      className="modal-formats"
      onClose={onClose}
      tools={<>
        <div className="search">
          <i className="fa-solid fa-magnifying-glass" aria-hidden="true" />
          <input
            type="search"
            value={query}
            placeholder="Type a file type, like MKV or PSD"
            aria-label="Search the formats"
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Escape' && query) { e.stopPropagation(); setQuery(''); } }}
          />
          {query && (
            <button type="button" className="icon-btn icon-btn-sm" aria-label="Clear" onClick={(e) => { setQuery(''); (e.currentTarget.previousElementSibling as HTMLElement).focus(); }}>
              <i className="fa-solid fa-xmark" aria-hidden="true" />
            </button>
          )}
        </div>
        <Seg<Kind | 'all'> label="Kind" value={tab} onChange={setTab} options={[{ value: 'all', label: 'All' }, ...KINDS.map((k) => ({ value: k.kind, label: k.title }))]} />
      </>}
    >
      {shown.length ? shown.map((k) => (
        <section key={k.kind} className="fmt-kind">
          <h4><i className={`fa-solid ${k.icon}`} aria-hidden="true" />{k.title}<span>become {k.becomes}</span></h4>
          <ul>
            {k.rows.map((f) => (
              <li key={f.name} className={q && f.ext.includes(q) ? 'hit' : undefined}>
                <b>{f.name}</b>
                <span className="ext">{f.ext.map((e) => '.' + e).join('  ')}</span>
                {f.note && <span className="badge">{f.note}</span>}
              </li>
            ))}
          </ul>
          {k.kind === 'video' && !q && formats.videoNote && <p className="fmt-note">{formats.videoNote}</p>}
        </section>
      )) : (
        <div className="fmt-none">
          <i className="fa-solid fa-file-circle-xmark" aria-hidden="true" />
          <h4>{/^[a-z0-9]{1,5}$/.test(q) ? `.${q} isn’t supported` : 'Nothing matches'}</h4>
          <p>Sastra Compressor can’t read this type here. Export it from your editor as one of the formats in this list.</p>
        </div>
      )}
    </Modal>
  );
}
