import { useRef, useState } from 'react';
import { Menu, useMenu } from './Menu';
import { TitleBar } from './TitleBar';
import { applyTheme, savedTheme, type ThemeChoice } from '../lib/theme';
import black from '../assets/wordmark-black.svg';
import white from '../assets/wordmark-white.svg';

const THEMES: { value: ThemeChoice; label: string; icon: string }[] = [
  { value: 'auto', label: 'Automatic', icon: 'fa-circle-half-stroke' },
  { value: 'light', label: 'Light', icon: 'fa-sun' },
  { value: 'dark', label: 'Dark', icon: 'fa-moon' },
];

/** This app's contents for the title bar. */
export function Header({ onAbout, inert }: { onAbout(): void; inert: boolean }) {
  const [theme, setTheme] = useState(savedTheme());
  const menu = useMenu();
  const btn = useRef<HTMLButtonElement>(null);
  const current = THEMES.find((t) => t.value === theme)!;

  const pick = (t: ThemeChoice) => {
    setTheme(t);
    applyTheme(t, true);
    menu.close();
  };

  return (
    <TitleBar
      inert={inert}
      start={
        <h1 className="app-name">
          <img className="logo-light" src={black} alt="Sastra™" draggable={false} />
          <img className="logo-dark" src={white} alt="" aria-hidden="true" draggable={false} />
          <span>Compressor</span>
        </h1>
      }
      end={<>
        <button type="button" className="icon-btn icon-btn-sm" aria-label="About" data-tip="About" onClick={onAbout}>
          <i className="fa-solid fa-circle-info" aria-hidden="true" />
        </button>
        <button
          ref={btn}
          type="button"
          className="icon-btn icon-btn-sm"
          aria-label="Theme"
          data-tip="Theme"
          aria-haspopup="menu"
          aria-expanded={menu.open}
          onClick={menu.toggle}
        >
          <i className={`fa-solid ${current.icon}`} aria-hidden="true" />
        </button>
        <Menu anchor={btn} menu={menu}>
          {THEMES.map((t) => (
            <button
              key={t.value}
              type="button"
              role="menuitemradio"
              aria-checked={t.value === theme}
              className={`menu-item${t.value === theme ? ' current' : ''}`}
              onClick={() => pick(t.value)}
            >
              <i className={`fa-solid ${t.icon}`} aria-hidden="true" />
              {t.label}
            </button>
          ))}
        </Menu>
      </>}
    />
  );
}
