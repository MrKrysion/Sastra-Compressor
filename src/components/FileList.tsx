import { memo } from 'react';
import type { Row } from '../lib/types';
import { bytes, percent } from '../lib/text';

const ICON = { image: 'fa-image', audio: 'fa-music', video: 'fa-film' } as const;

interface Props {
  rows: Row[];
  /** Where each file will be saved (by its path), from the backend's plan. */
  outputs: Record<string, string>;
  busy: boolean;
  onRemove(path: string): void;
  onReveal(path: string): void;
}

export function FileList({ rows, outputs, busy, onRemove, onReveal }: Props) {
  return (
    <ul className="files" aria-label="Files to compress">
      {rows.map((r) => (
        <FileRow key={r.path} row={r} target={outputs[r.path] ?? ''} busy={busy} onRemove={onRemove} onReveal={onReveal} />
      ))}
    </ul>
  );
}

const FileRow = memo(function FileRow({ row, target, busy, onRemove, onReveal }: {
  row: Row; target: string; busy: boolean; onRemove(p: string): void; onReveal(p: string): void;
}) {
  const s = row.status;
  const showRel = row.rel !== row.name;
  let meta: React.ReactNode;
  let cls = 'file-row';

  if (s.state === 'waiting') {
    const name = target.split(/[\\/]/).pop();
    meta = (
      <small>
        {bytes(row.size)}
        {name && <> <i className="fa-solid fa-arrow-right arrow" aria-label="becomes" /> <span className="target" data-tip={target}>{name}</span></>}
      </small>
    );
  } else if (s.state === 'running') {
    meta = (
      <>
        <span className="progress" role="progressbar" aria-valuenow={Math.round(s.progress * 100)} aria-valuemin={0} aria-valuemax={100}>
          <span style={{ width: `${Math.max(2, s.progress * 100)}%` }} />
        </span>
        <small>{s.progress > 0 ? `${percent(s.progress)} · ` : 'Starting · '}{bytes(row.size)}</small>
      </>
    );
    cls += ' running';
  } else if (s.state === 'done') {
    const saved = 1 - s.outSize / Math.max(1, row.size);
    cls += ' done';
    meta = saved >= 0 ? (
      <small>{bytes(row.size)} <i className="fa-solid fa-arrow-right arrow" aria-label="became" /> {bytes(s.outSize)} · <b className="saved">{percent(saved)} smaller</b></small>
    ) : (
      <small className="warn-text">
        <i className="fa-solid fa-triangle-exclamation" aria-hidden="true" /> {bytes(s.outSize)}, {percent(-saved)} bigger than the original
      </small>
    );
  } else {
    const [message, detail] = s.error.split('\n');
    cls += ' failed';
    meta = (
      <small className="err-text" data-tip={detail} data-tip-always={detail ? '' : undefined}>
        <i className="fa-solid fa-circle-exclamation" aria-hidden="true" /> {message}
      </small>
    );
  }

  return (
    <li className={cls}>
      <i className={`fa-solid ${ICON[row.kind]} file-icon`} aria-hidden="true" />
      <div className="file-main">
        <b title={row.path}>
          {showRel && <span className="file-dir">{row.rel.slice(0, row.rel.length - row.name.length)}</span>}
          {row.name}
        </b>
        {meta}
      </div>
      {s.state === 'done' ? (
        <>
          <button type="button" className="icon-btn" aria-label="Show in folder" data-tip="Show in folder" onClick={() => onReveal(s.outPath)}>
            <i className="fa-solid fa-folder-open" aria-hidden="true" />
          </button>
          <span className="file-done" aria-label="Done"><i className="fa-solid fa-circle-check" aria-hidden="true" /></span>
        </>
      ) : (
        <button type="button" className="icon-btn" aria-label="Remove" data-tip="Remove" disabled={busy} onClick={() => onRemove(row.path)}>
          <i className="fa-solid fa-xmark" aria-hidden="true" />
        </button>
      )}
    </li>
  );
});
