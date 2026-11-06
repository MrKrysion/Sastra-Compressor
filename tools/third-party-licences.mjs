// Writes THIRD-PARTY-LICENCES.txt: the licence text of every Rust crate and npm package
// that goes into the Windows app. CI runs this for every build and ships the result in the
// `licences` folder; it isn't kept in the repository. Needs `npm ci` and Rust.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const ROOT = path.join(import.meta.dirname, '..');
const NAME = /^(licen[cs]e|copying|notice|patents|unlicense|copyright)/i;

/** Licence files in a package, and in its folders of bundled C sources (two levels down). */
function licenceFiles(dir, depth = 0) {
  const found = [];
  for (const e of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
    const p = path.join(dir, e.name);
    if (e.isFile() && NAME.test(e.name) && !/\.(rs|js|mjs|ts|h|c|cc|toml|json|py|sh|svg|png)$/i.test(e.name)) found.push(p);
    else if (e.isDirectory() && depth < 2 && !/^(tests?|examples?|benches|node_modules|target|\.git(hub)?|docs?)$/i.test(e.name)) found.push(...licenceFiles(p, depth + 1));
  }
  return found;
}
const read = (p) => fs.readFileSync(p, 'utf8').replace(/\r\n/g, '\n').trim();

function rustPackages() {
  const meta = JSON.parse(execFileSync('cargo', [
    'metadata', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc',
    '--manifest-path', path.join(ROOT, 'src-tauri', 'Cargo.toml'), '--locked',
  ], { encoding: 'utf8', maxBuffer: 1 << 29 }));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const packages = new Map(meta.packages.map((p) => [p.id, p]));
  // Only what is linked into the app: not build tools, not test tools.
  const seen = new Set();
  const walk = (id) => {
    if (seen.has(id)) return;
    seen.add(id);
    for (const d of nodes.get(id).deps) {
      if (d.dep_kinds.some((k) => k.kind === null)) walk(d.pkg);
    }
  };
  walk(meta.resolve.root);
  seen.delete(meta.resolve.root);
  return [...seen].map((id) => packages.get(id)).map((p) => ({
    name: p.name, version: p.version, licence: p.license ?? `see ${p.license_file}`, source: p.repository ?? '',
    authors: p.authors, files: licenceFiles(path.dirname(p.manifest_path)),
  }));
}

function npmPackages() {
  const lock = JSON.parse(fs.readFileSync(path.join(ROOT, 'package-lock.json'), 'utf8'));
  return Object.entries(lock.packages)
    .filter(([p, info]) => p && !info.dev && !info.optional && !info.devOptional)
    .map(([p, info]) => {
      const dir = path.join(ROOT, p);
      return {
        name: p.replace(/^.*node_modules\//, ''), version: info.version, licence: info.license ?? 'see below',
        source: JSON.parse(fs.readFileSync(path.join(dir, 'package.json'), 'utf8')).repository?.url ?? '',
        authors: [], files: licenceFiles(dir),
      };
    });
}

// Published without their own copy of the licence, but by the same authors as a package that has it.
const SAME_LICENCE_AS = { 'alloc-stdlib': 'alloc-no-stdlib' };

const all = [...rustPackages(), ...npmPackages()].sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));

for (const p of all) {
  const twin = SAME_LICENCE_AS[p.name] && all.find((o) => o.name === SAME_LICENCE_AS[p.name]);
  if (twin && !p.files.length) p.files = twin.files;
}

// Standard texts, taken from packages that do include them, for those that only name a licence.
const standard = {};
for (const p of all) {
  for (const f of p.files) {
    const t = read(f);
    if (!standard['Apache-2.0'] && /Apache License\s+Version 2\.0/.test(t) && /END OF TERMS/.test(t)) standard['Apache-2.0'] = t;
    if (!standard['MPL-2.0'] && /^Mozilla Public License,? Version 2\.0/i.test(t)) standard['MPL-2.0'] = t;
    if (!standard.MIT && /^(The )?MIT License/i.test(t)) standard.MIT = t.replace(/^.*Copyright.*$/m, 'Copyright (c) the authors named above');
  }
}

// Many packages share a text, so each distinct one is printed once and referred to by number.
const texts = new Map();
const refer = (text) => {
  if (!texts.has(text)) texts.set(text, texts.size + 1);
  return texts.get(text);
};
const list = [];
const missing = [];
for (const p of all) {
  const lines = [`${p.name} ${p.version}`, `  Licence: ${p.licence}`];
  if (p.source) lines.push(`  Source: ${p.source}`);
  if (p.authors.length) lines.push(`  Authors: ${p.authors.join(', ')}`);
  for (const f of p.files) lines.push(`  ${path.basename(f)}: text ${refer(read(f))}`);
  if (!p.files.length) {
    const ids = Object.keys(standard).filter((id) => p.licence.includes(id));
    if (!ids.length) missing.push(`${p.name} ${p.version} (${p.licence})`);
    for (const id of ids) lines.push(`  Standard ${id} terms: text ${refer(standard[id])}`);
  }
  list.push(lines.join('\n'));
}
if (missing.length) {
  console.error(`No licence text found for:\n  ${missing.join('\n  ')}`);
  process.exit(1);
}

const out = [
  'Third-party licences for Sastra Compressor',
  '',
  'Every Rust crate and npm package built into the app, followed by their licence texts.',
  'FFmpeg and what it is built with are in the other files of the licences folder; see THIRD-PARTY-NOTICES.md.',
  'Where a package offers a choice of licences, all of them are listed, and they are used under the MIT licence where that is one of the choices.',
  'This file is generated by tools/third-party-licences.mjs.',
  '',
  '='.repeat(72),
  'PACKAGES',
  '='.repeat(72),
  '',
  list.join('\n\n'),
  '',
];
for (const [text, n] of texts) out.push('='.repeat(72), `TEXT ${n}`, '='.repeat(72), '', text, '');
fs.writeFileSync(path.join(ROOT, 'THIRD-PARTY-LICENCES.txt'), out.join('\n'));
console.log(`${all.length} packages, ${texts.size} distinct licence texts`);
