import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
// Everything is bundled, so the app works offline.
import '@fortawesome/fontawesome-free/css/fontawesome.min.css';
import '@fortawesome/fontawesome-free/css/solid.min.css';
import '@fortawesome/fontawesome-free/css/brands.min.css';
import './base.css';
import './app.css';
import { applyTheme, savedTheme } from './lib/theme';
import { backend } from './lib/backend';
import App from './App';
import { behaveLikeAnApp } from './lib/app-behaviour';
import { initUi } from './lib/ui';

applyTheme(savedTheme());
behaveLikeAnApp();
initUi();

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

// The window starts hidden so it never flashes white; show it once the first frame is drawn.
// (A timer, not an animation frame: a hidden window may never draw one.)
setTimeout(() => backend.then((b) => b.show()).catch(() => {}), 30);
