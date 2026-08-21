import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { App } from '../../App';
import type { ScreenMap } from '../../ui/screens';
import { discoverScreens } from '../../ui/screens';
import { NavIcon } from '../../ui/shell/nav-icons';
import type { Section } from '../../ui/sections';
import { SECTIONS, sectionEntry } from '../../ui/sections';
import TriggersScreen from './index';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const SIX = ['run', 'workflows', 'agents', 'skills', 'memory', 'triggers'] as const;

function textOf(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

function withoutRemarks(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/.*$/gm, ' ');
}

function arrayBody(source: string, name: string): string {
  const clean = withoutRemarks(source);
  const start = clean.indexOf(`const ${name}`);
  const open = clean.indexOf('[', start);
  if (start < 0 || open < 0) return '';
  let depth = 0;
  for (let at = open; at < clean.length; at += 1) {
    const char = clean[at];
    if (char === '[') depth += 1;
    if (char === ']') {
      depth -= 1;
      if (depth === 0) return clean.slice(open + 1, at);
    }
  }
  return '';
}

function literals(body: string): string[] {
  return [...body.matchAll(/['"]([a-z-]+)['"]/g)].map((hit) => hit[1] ?? '');
}

describe('Triggers is the sixth real section', () => {
  it('registers the English label and one short empty sentence', () => {
    expect(SECTIONS.map((entry) => entry.id)).toEqual(SIX);
    const entry = SECTIONS.find((one) => one.id === ('triggers' as Section));
    expect(entry?.label).toBe('Triggers');
    expect(entry?.empty.split(/\s+/).filter(Boolean).length).toBeLessThanOrEqual(12);
    expect((entry?.empty.match(/\./g) ?? []).length).toBeLessThanOrEqual(1);
    expect(sectionEntry('triggers' as Section)).toBe(entry);
  });

  it('discovers this module and App renders this screen on the production route', () => {
    const screens = discoverScreens() as ScreenMap & { readonly triggers?: unknown };
    expect(screens.triggers).toBe(TriggersScreen);
    const markup = renderToStaticMarkup(
      <App section={'triggers' as Section} screens={screens as ScreenMap} />,
    );
    expect(markup).toContain('data-section="triggers"');
    expect(markup).toContain('data-triggers-screen');
    const empty = /<[^>]+data-empty[^>]*>([\s\S]*?)<\//.exec(markup)?.[1] ?? '';
    expect(empty.replace(/<[^>]*>/g, '').trim()).toBe(sectionEntry('triggers' as Section).empty);
  });

  it('uses a nonempty currentColor glyph without circles or decorative lines', () => {
    const markup = renderToStaticMarkup(<NavIcon section="triggers" />);
    expect(markup).toContain('<svg');
    expect(markup).toContain('aria-hidden="true"');
    expect(markup).toContain('currentColor');
    expect(markup).not.toContain('<circle');
    expect(markup).not.toContain('<line');
    expect(markup).toMatch(/<(?:path|rect|polyline|polygon)\b/);
  });

  it('raises each of the five independent section mirrors to exactly the same six ids', () => {
    const mirrors = [
      ['src/ui/shell/controls.test.tsx', 'EXPECTED'],
      ['src/ui/shell/screen-mount.test.tsx', 'EXPECTED'],
      ['src/ui/shell/screen-fallback.test.tsx', 'EXPECTED'],
      ['src/sections/empty-screen-invites.test.tsx', 'FIVE'],
    ] as const;
    for (const [path, constant] of mirrors) {
      const body = arrayBody(textOf(resolve(ROOT, path)), constant);
      expect(literals(body), path + ' must name all six sections independently').toEqual(SIX);
    }

    const sections = arrayBody(textOf(resolve(ROOT, 'src/ui/shell/sections.test.tsx')), 'EXPECTED');
    const ids = [...sections.matchAll(/\bid\s*:\s*['"]([a-z-]+)['"]/g)].map((hit) => hit[1] ?? '');
    expect(ids).toEqual(SIX);
  });

  it('adds the new list screen to the shared radius-band oracle', () => {
    const body = arrayBody(
      textOf(resolve(ROOT, 'src/sections/radii-band-reaches-the-sections.test.tsx')),
      'SECTIONS',
    );
    expect(literals(body)).toEqual(['agents', 'skills', 'memory', 'workflows', 'triggers']);
  });
});
