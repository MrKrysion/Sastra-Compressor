// A menu that grows from its button and plays its close animation.
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';

export function useMenu() {
  const [open, setOpen] = useState(false);
  const [closing, setClosing] = useState(false);
  const close = () => {
    if (!open || closing) return;
    setClosing(true);
    setTimeout(() => { setOpen(false); setClosing(false); }, 150);
  };
  return { open, closing, toggle: () => (open ? close() : setOpen(true)), close };
}

export function Menu({ anchor, menu, children }: { anchor: React.RefObject<HTMLElement | null>; menu: ReturnType<typeof useMenu>; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ top: 0, right: 0 });

  useLayoutEffect(() => {
    if (!menu.open || !anchor.current) return;
    const r = anchor.current.getBoundingClientRect();
    setPos({ top: r.bottom + 8, right: document.documentElement.clientWidth - r.right });
  }, [menu.open, anchor]);

  const closeRef = useRef(menu.close);
  closeRef.current = menu.close;

  // While open: a click elsewhere, Escape, or resizing closes it. Focus moves in once, on opening.
  useEffect(() => {
    if (!menu.open) return;
    const close = () => closeRef.current();
    const away = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node) && !anchor.current?.contains(e.target as Node)) close();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { close(); anchor.current?.focus(); }
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        const items = [...(ref.current?.querySelectorAll<HTMLElement>('.menu-item') ?? [])];
        const i = items.indexOf(document.activeElement as HTMLElement);
        items[(i + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length]?.focus();
        e.preventDefault();
      }
    };
    document.addEventListener('pointerdown', away);
    document.addEventListener('keydown', key);
    window.addEventListener('resize', close);
    ref.current?.querySelector<HTMLElement>('[aria-checked="true"], .menu-item')?.focus();
    return () => {
      document.removeEventListener('pointerdown', away);
      document.removeEventListener('keydown', key);
      window.removeEventListener('resize', close);
    };
  }, [menu.open, anchor]);

  if (!menu.open) return null;
  return (
    <div
      ref={ref}
      role="menu"
      className={`menu end${menu.closing ? ' closing' : ''}`}
      style={{ position: 'fixed', top: pos.top, right: pos.right, zIndex: 'var(--z-float)' as never }}
    >
      {children}
    </div>
  );
}
