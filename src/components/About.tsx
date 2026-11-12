import { useEffect, useState } from 'react';
import { Modal } from './Modal';
import { backend } from '../lib/backend';
import black from '../assets/wordmark-black.svg';
import white from '../assets/wordmark-white.svg';

const REPO = 'https://github.com/MrKrysion/Sastra-Compressor';

const CREDITS: [string, string, string][] = [
  ['FFmpeg', 'GNU LGPL 2.1 or later', 'https://ffmpeg.org'],
  ['libvpx', 'BSD 3-clause', 'https://www.webmproject.org'],
  ['Opus', 'BSD', 'https://opus-codec.org'],
  ['dav1d', 'BSD', 'https://code.videolan.org/videolan/dav1d'],
  ['libwebp', 'BSD 3-clause', 'https://chromium.googlesource.com/webm/libwebp'],
  ['Tauri', 'MIT or Apache 2.0', 'https://tauri.app'],
  ['React', 'MIT', 'https://react.dev'],
  ['Gabarito', 'SIL Open Font Licence 1.1', 'https://github.com/naipefoundry/gabarito'],
  ['Font Awesome Free', 'CC BY 4.0, SIL OFL 1.1, and MIT', 'https://fontawesome.com'],
];

export function About({ onClose }: { onClose(): void }) {
  const [version, setVersion] = useState('');
  useEffect(() => { backend.then((b) => b.version()).then(setVersion); }, []);
  const open = (url: string) => backend.then((b) => b.openUrl(url));

  return (
    <Modal title="About" icon="fa-circle-info" size="content" onClose={onClose}>
      <div className="about">
        <div className="about-main">
          <img className="logo-light about-logo" src={black} alt="Sastra™" />
          <img className="logo-dark about-logo" src={white} alt="" aria-hidden="true" />
          <p className="about-name">Compressor {version && <span className="meta">version {version}</span>}</p>
          <p>Compresses images, audio, and video into small, web-friendly files for the Sastra™ visual novel engine.</p>
          <div className="btn-row">
            <button type="button" className="btn btn-secondary btn-sm" onClick={() => open('https://www.sastra.dev')}>
              <i className="fa-solid fa-globe" aria-hidden="true" />Sastra™ website
            </button>
            <button type="button" className="btn btn-secondary btn-sm" onClick={() => open(REPO)}>
              <i className="fa-brands fa-github" aria-hidden="true" />Source code
            </button>
          </div>
          <div className="about-small">
            <p>
              The bundled FFmpeg is free software under the GNU LGPL. Its source code and licences come with every{' '}
              <a href={REPO + '/releases'} onClick={(e) => { e.preventDefault(); open(REPO + '/releases'); }}>release</a>.
            </p>
            <p>© 2026 Ken “Krysion” Nisaka</p>
          </div>
        </div>
        <div className="about-credits">
          <h4 className="eyebrow">Made with</h4>
          <ul className="credits">
            {CREDITS.map(([name, licence, url]) => (
              <li key={name}>
                <a href={url} onClick={(e) => { e.preventDefault(); open(url); }}>{name}</a>
                <span className="meta">{licence}</span>
              </li>
            ))}
          </ul>
        </div>
      </div>
    </Modal>
  );
}
