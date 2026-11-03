# The Krysion design language

`krysion.css`, `krysion.js`, and the Gabarito font, copied here so the app works offline. These files are © Ken “Krysion” Nisaka and are **not** covered by this repository's MIT licence; Gabarito is under the SIL Open Font Licence (`fonts/OFL.txt`).

To update them, copy the new versions over these, then change two lines of `krysion.css`:

1. Remove the Font Awesome `@import` line at the top. The app bundles Font Awesome from npm instead (`src/main.tsx`), so keep the version in `package.json` the same as the one in that line.
2. Point the Gabarito `src: url(...)` at `./fonts/Gabarito-VariableFont_wght.woff2`.
