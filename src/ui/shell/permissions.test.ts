/* Kryterium 2 dla T-01: zasięg webviewa jest zamknięty i celuje w okno, które istnieje.
 *
 * Czytamy WSZYSTKIE pliki `*.json` z src-tauri/capabilities/, nie sam default.json. Tauri włącza
 * cały katalog, o ile tauri.conf.json nie wymienia plików po nazwie, więc asercja na jednym pliku
 * przechodzi, gdy drugi przyznaje `core:default`.
 *
 * Trzy rzeczy odróżniają zamknięty zasięg od zasięgu, który tylko tak wygląda:
 *   suma po CAŁYM katalogu, przynależność do zamkniętej listy z T8 §3.2, i porównanie pola
 *   `windows` z etykietą okna z DRUGIEGO pliku. To ostatnie jest najcichsze: `["Loadout"]` przy
 *   oknie o etykiecie `main` to permissions, które nie dotyczą niczego — a webview dowie się
 *   o tym dopiero w T-07, trzy zadania później, i przeczyta to jako błąd wywołania.
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''` (AGENTS.md §2a p. 5).
 */
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');

/** Zamknięta lista z T8 §3.2. Wszystko poza nią wymaga decyzji człowieka, nie commita. */
const ALLOWED = [
  'core:event:default',
  'core:app:default',
  'core:window:allow-start-dragging',
  'core:window:allow-close',
  'core:window:allow-set-focus',
  'dialog:allow-open',
  'store:default',
  'opener:allow-reveal-item-in-dir',
];

const CSP_RULE = "default-src 'self'";

/** Wtyczki, które dają webviewowi uruchamianie poleceń i dysk. T8 §3.3: nigdy. */
const BANNED_CRATES = ['tauri-plugin-shell', 'tauri-plugin-fs'];

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

function fileJson(path: string): unknown {
  const raw = fileText(path);
  if (raw.trim() === '') return undefined;
  try {
    return JSON.parse(raw) as unknown;
  } catch {
    return undefined;
  }
}

function at(value: unknown, ...path: readonly string[]): unknown {
  let cursor: unknown = value;
  for (const key of path) {
    if (typeof cursor !== 'object' || cursor === null) return undefined;
    cursor = (cursor as Record<string, unknown>)[key];
  }
  return cursor;
}

function shown(value: unknown): string {
  return value === undefined ? '<nothing there>' : JSON.stringify(value);
}

/** Uprawnienie bywa napisem albo obiektem `{ identifier, allow }`. Obie formy liczą się tak samo. */
function identifiers(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  const out: string[] = [];
  for (const item of value) {
    if (typeof item === 'string') {
      out.push(item);
      continue;
    }
    const id = at(item, 'identifier');
    if (typeof id === 'string') out.push(id);
  }
  return out;
}

const capabilitiesDir = resolve(ROOT, 'src-tauri', 'capabilities');
const capabilityFiles = existsSync(capabilitiesDir)
  ? readdirSync(capabilitiesDir)
      .filter((name) => name.endsWith('.json'))
      .sort()
  : [];
const capabilities = capabilityFiles.map((name) => ({
  name,
  body: fileJson(resolve(capabilitiesDir, name)),
}));
const granted = capabilities.flatMap((file) => identifiers(at(file.body, 'permissions')));

const conf = fileJson(resolve(ROOT, 'src-tauri', 'tauri.conf.json'));
const windows = at(conf, 'app', 'windows');
const windowLabel = at(Array.isArray(windows) ? windows[0] : undefined, 'label');

/* Komentarze zdejmujemy PRZED szukaniem nazw wtyczek. src-tauri/Cargo.toml niesie linię
 * „ŚWIADOMIE BRAK: tauri-plugin-shell, tauri-plugin-fs", więc `includes('tauri-plugin-shell')`
 * na surowym pliku przechodzi na komentarzu i nie zauważa żywej zależności — to jest dokładnie
 * ta pomyłka, którą AGENTS.md §20 opisuje na `--sandbox workspace-write` w spreadsheecie.
 * Czego to NIE widzi: znaku `#` wewnątrz wartości napisowej. W tym pliku takiej nie ma.
 */
const cargo = fileText(resolve(ROOT, 'src-tauri', 'Cargo.toml'))
  .split('\n')
  .map((line) => line.replace(/#.*$/, ''))
  .join('\n');

function dependsOn(crate: string): boolean {
  const asKey = new RegExp('^\\s*"?' + crate + '"?\\s*(?:=|\\.)', 'm');
  const asTable = new RegExp('^\\s*\\[[^\\]\\n]*\\.' + crate + '\\s*\\]', 'm');
  return asKey.test(cargo) || asTable.test(cargo);
}

describe('the reach of the webview is closed and points at a window that exists', () => {
  it('has at least one permissions file, and it grants something', () => {
    expect(
      capabilityFiles.length,
      'src-tauri/capabilities/ has to hold at least one .json file. On an empty directory every ' +
        'assertion below is true of nothing at all',
    ).toBeGreaterThanOrEqual(1);
    expect(
      granted.length,
      'the permissions across that directory have to add up to something. An empty list also ' +
        'passes every ban below, and it is not what a working app looks like',
    ).toBeGreaterThanOrEqual(1);
  });

  it('never grants core:default', () => {
    expect(
      granted,
      'core:default drags in core:image, core:menu, core:path, core:resources, core:tray and ' +
        'core:webview, none of which this app calls. T8 §3.2 is blunt about it: enumerate the ' +
        'handful you use. The directory grants: ' +
        shown(granted),
    ).not.toContain('core:default');
  });

  it('gives the webview neither command running nor the disk', () => {
    const reaching = granted.filter((id) => id.startsWith('shell:') || id.startsWith('fs:'));
    expect(
      reaching,
      'one flaw in a markdown renderer — and this app renders agent-written markdown in the ' +
        'Memory section — turns a shell permission into arbitrary code running on the machine. ' +
        'Rust does the running, the webview asks for it by name (T8 §3.3)',
    ).toEqual([]);
    for (const crate of BANNED_CRATES) {
      expect(
        dependsOn(crate),
        'src-tauri/Cargo.toml has to stay free of ' +
          crate +
          '. Shipping the crate and withholding the permission is one edit away from granting it',
      ).toBe(false);
    }
  });

  it('grants nothing outside the closed list', () => {
    const outside = granted.filter((id) => !ALLOWED.includes(id));
    expect(
      outside,
      'every permission has to come from the list in T8 §3.2. Anything else is a new hole in ' +
        'the webview, and a new hole is a decision a person makes, not a line a commit adds. ' +
        'The list is: ' +
        ALLOWED.join(', '),
    ).toEqual([]);
  });

  it('points every permissions file at the window that actually exists', () => {
    expect(
      typeof windowLabel === 'string' && windowLabel.length > 0,
      'tauri.conf.json has to name the window before any permissions file can match it; it ' +
        'says: ' +
        shown(windowLabel),
    ).toBe(true);
    for (const file of capabilities) {
      const targets = at(file.body, 'windows');
      expect(
        Array.isArray(targets) ? targets : undefined,
        'src-tauri/capabilities/' +
          file.name +
          ' has to carry a windows field. Tauri matches it on the window LABEL, not the shown ' +
          'name; it says: ' +
          shown(targets),
      ).toBeInstanceOf(Array);
      expect(
        Array.isArray(targets) ? targets : [],
        'src-tauri/capabilities/' +
          file.name +
          ' names a window that does not exist, so it grants nothing and every call from the ' +
          'webview is refused — which T-07 will read as a broken call, three tasks from here. ' +
          'The window is called ' +
          shown(windowLabel) +
          ' and the file says: ' +
          shown(targets),
      ).toContain(windowLabel);
    }
  });

  it('closes the page down to what we ship', () => {
    const csp = at(conf, 'app', 'security', 'csp');
    expect(
      typeof csp === 'string' && csp.trim().length > 0,
      'app.security.csp has to be a real rule. null means anything the page asks for is fetched, ' +
        'and this app is meant to work with the network unplugged; it says: ' +
        shown(csp),
    ).toBe(true);
    expect(
      typeof csp === 'string' ? csp : '',
      'the rule has to hold ' + CSP_RULE + ', so nothing outside the bundle can be loaded',
    ).toContain(CSP_RULE);
  });
});
