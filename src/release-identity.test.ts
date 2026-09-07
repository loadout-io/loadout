import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/* 2026-08-31: brak pliku zwraca pusty tekst, żeby pierwszy przebieg padał na asercji
 * kontraktu, a nie podczas zbierania testu. */
const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

function tomlString(source: string, section: string, key: string): string | undefined {
  const header = `[${section}]`;
  const lines = source.split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim() === header);
  if (start < 0) return undefined;

  for (const line of lines.slice(start + 1)) {
    if (/^\s*\[/.test(line)) return undefined;
    const entry = line.match(/^\s*([A-Za-z0-9_-]+)\s*=\s*"([^"]*)"\s*(?:#.*)?$/);
    if (entry?.[1] === key) return entry[2];
  }
  return undefined;
}

describe('release identity', () => {
  /* CO PODPISUJEMY, DECYDUJE O TYM, CZY FUNKCJA W OGOLE DZIALA U CZLOWIEKA.
   *
   * Krok QA sprawdza, czy uruchomiona aplikacja ma okno, przez `osascript` rozmawiajacy
   * z „System Events" — czyli Apple eventem. Wydanie 0.2.x bylo podpisane hardened runtime
   * z ZEREM uprawnien (`codesign -d --entitlements -` na zainstalowanym .app oddawalo pustke),
   * wiec ta droga u uzytkownika NIE ISTNIALA: `native_ui` odpowiadal `NotPermitted`, a wynik
   * schodzil na „nie zmierzono". Poprawnie i bezuzytecznie zarazem — dokladnie ten rodzaj
   * funkcji, ktora przechodzi kazdy test i jest martwa po spakowaniu.
   *
   * Potrzebne sa OBIE rzeczy naraz i dlatego stoja w jednym kryterium: uprawnienie zdejmuje
   * blokade hardened runtime, a opis w Info.plist jest zdaniem, ktore macOS pokaze czlowiekowi,
   * gdy zapyta o zgode. Bez opisu system nie ma czego wyswietlic. */
  it('ships the automation entitlement and the sentence macOS shows when it asks', () => {
    const config = JSON.parse(text(resolve(ROOT, 'src-tauri', 'tauri.conf.json'))) as {
      bundle?: { macOS?: { entitlements?: string; infoPlist?: string } };
    };
    const entitlementsPath = config.bundle?.macOS?.entitlements;
    expect(
      entitlementsPath,
      'bundle.macOS.entitlements is unset, so the signed app carries no entitlements at all ' +
        'and the step that checks a running app can only ever answer "not tested"',
    ).toBeTruthy();

    const entitlements = text(resolve(ROOT, 'src-tauri', entitlementsPath ?? ''));
    expect(
      entitlements,
      'the entitlements file does not grant com.apple.security.automation.apple-events, ' +
        'so hardened runtime keeps refusing the only route to a native window',
    ).toContain('com.apple.security.automation.apple-events');

    const infoPlist = text(resolve(ROOT, 'src-tauri', config.bundle?.macOS?.infoPlist ?? ''));
    expect(
      infoPlist,
      'Info.plist has no NSAppleEventsUsageDescription, so macOS has no sentence to show ' +
        'the person it is asking for permission',
    ).toContain('NSAppleEventsUsageDescription');
  });

  it('keeps the release identity aligned across manifests', () => {
    const packageJson = JSON.parse(text(resolve(ROOT, 'package.json'))) as { version?: string };
    const tauriConfig = JSON.parse(text(resolve(ROOT, 'src-tauri', 'tauri.conf.json'))) as {
      version?: string;
      identifier?: string;
      bundle?: { macOS?: { minimumSystemVersion?: string } };
    };
    const cargoManifest = text(resolve(ROOT, 'src-tauri', 'Cargo.toml'));
    const cargoConfig = text(resolve(ROOT, '.cargo', 'config.toml'));
    const cargoVersion = tomlString(cargoManifest, 'package', 'version');
    const deploymentTarget = tomlString(cargoConfig, 'env', 'MACOSX_DEPLOYMENT_TARGET');
    const minimumSystemVersion = tauriConfig.bundle?.macOS?.minimumSystemVersion;

    expect(
      deploymentTarget,
      'MACOSX_DEPLOYMENT_TARGET in .cargo/config.toml must match ' +
        'bundle.macOS.minimumSystemVersion in src-tauri/tauri.conf.json',
    ).toBe(minimumSystemVersion);
    expect(tauriConfig.identifier).toBe('com.loadout.desktop');
    expect([packageJson.version, cargoVersion, tauriConfig.version]).toEqual([
      packageJson.version,
      packageJson.version,
      packageJson.version,
    ]);
  });
});
