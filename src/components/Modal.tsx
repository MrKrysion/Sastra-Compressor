// A dialog: closes with ✕, the backdrop, or Escape, and
// always plays its closing animation.
import { useEffect, useRef, useState, type ReactNode } from 'react';

interface Props {
  title: string;
  icon?: string;
  sub?: string;
  size?: 'confirm' | 'content';
  onClose(): void;
  foot?: ReactNode;
  children: ReactNode;
}

export function Modal({ title, icon, sub, size, onClose, foot, children }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [closing, setClosing] = useState(false);
  const closingRef = useRef(false);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const close = () => {
    if (closingRef.current) return;
    closingRef.current = true;
    setClosing(true);
    setTimeout(() => onCloseRef.current(), 200);
  };

  // Once, on opening: focus the dialog (never a field), Escape closes it, the page stays put.
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    ref.current?.focus();
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); close(); } };
    document.addEventListener('keydown', key);
    document.body.classList.add('modal-open');
    return () => {
      document.removeEventListener('keydown', key);
      document.body.classList.remove('modal-open');
      before?.focus?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const id = 'modal-' + title.replace(/\W+/g, '-').toLowerCase();
  return (
    <>
      <div className={`modal-backdrop open${closing ? ' closing' : ''}`} onClick={close} />
      <div
        ref={ref}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-labelledby={id}
        className={`modal open${size ? ' modal-' + size : ''}${closing ? ' closing' : ''}`}
      >
        <div className="modal-head">
          {sub ? (
            <hgroup>
              {icon && <i className={`fa-solid ${icon}`} aria-hidden="true" />}
              <h3 id={id}>{title}</h3>
              <p className="modal-sub">{sub}</p>
            </hgroup>
          ) : (
            <h3 id={id}>
              {icon && <i className={`fa-solid ${icon}`} aria-hidden="true" />}
              {title}
            </h3>
          )}
          <button type="button" className="icon-btn" aria-label="Close" data-tip="Close" onClick={close}>
            <i className="fa-solid fa-xmark" aria-hidden="true" />
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {foot && <div className="modal-foot">{foot}</div>}
      </div>
    </>
  );
}

/** Lets a button in the foot close the modal with its animation. */
export function closeModal(from: HTMLElement) {
  from.closest('.modal')?.querySelector<HTMLButtonElement>('.modal-head > .icon-btn')?.click();
}
