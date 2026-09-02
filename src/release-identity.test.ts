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
