import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
// The Krysion design language, bundled so the app works offline (see src/design/README.md).
import '@fortawesome/fontawesome-free/css/fontawesome.min.css';
import '@fortawesome/fontawesome-free/css/solid.min.css';
import '@fortawesome/fontawesome-free/css/brands.min.css';
import './design/krysion.css';
import './app.css';
import './design/krysion.js';
import { applyTheme, savedTheme } from './lib/theme';
import { backend } from './lib/backend';
import App from './App';
import { behaveLikeAnApp } from './lib/app-behaviour';

applyTheme(savedTheme());
behaveLikeAnApp();

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

// The window starts hidden so it never flashes white; show it once the first frame is drawn.
// (A timer, not an animation frame: a hidden window may never draw one.)
setTimeout(() => backend.then((b) => b.show()).catch(() => {}), 30);
