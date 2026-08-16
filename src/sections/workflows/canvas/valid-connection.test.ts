/* Kryterium 2 dla T-13: strzałka domykająca koło po prostu nie ląduje — bez komunikatu.
 *
 * Słaba wersja tego kryterium to pojedyncze `expect(isValidConnection(cAtoA)).toBe(false)`.
 * Przechodzi dla `() => false`, czyli dla płótna, na którym nie da się narysować ani jednej
 * strzałki, i nikt tego nie zauważy przez tydzień, bo „nie da się połączyć" wygląda tak samo
 * jak „to połączenie jest złe". Rozróżnia to przypadek `a → c`, który jest rombem, nie kołem,
 * i musi wrócić `true`.
 *
 * Druga połowa kryterium jest o tym, czego NIE MA na ekranie. Koło jest UNIEMOŻLIWIONE, nie
 * zgłoszone: uchwyt szarzeje, strzałka nie ląduje i nie pada ani jedno zdanie, bo użytkownik
 * nie zrobił nic złego [T3 §5.1]. Toast „cannot create cycle" jest tu regresją, nie ulepszeniem.
 */
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { AgentStep, WorkflowFile } from '../../../state/workflows';
import { isValidConnection, onConnect } from './connect';
import { RunBar } from './problems';

function step(id: string, name: string, y: number): AgentStep {
  return {
    kind: 'agent',
    id,
    name,
    agent: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    overrides: {},
    copies: 1,
    instructions: 'Do the part of the work this tile owns.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y },
  };
}

/** `a → b → c`, trzy kroki w łańcuchu. */
function file(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_ship_a_feature',
    name: 'Ship a feature',
    steps: [step('a', 'Plan', 24), step('b', 'Build', 168), step('c', 'Check', 312)],
    links: [
      { from: 'a', to: 'b' },
      { from: 'b', to: 'c' },
    ],
  };
}

function noop(): void {
  /* sterowany pasek: w statycznym renderze nic tego nie woła */
}

function plain(fragment: string): string {
  return fragment
    .replace(/<[^>]*>/g, ' ')
    .replace(/&#x27;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Atrybuty przycisku o tym napisie, albo `null`, kiedy takiego przycisku nie ma. */
function buttonAttributes(html: string, label: string): string | null {
  for (const hit of html.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)) {
    if (plain(hit[2] ?? '') === label) return hit[1] ?? '';
  }
  return null;
}

/** Pasek nad przyciskiem Run, wyrenderowany z uwag, których w tym pliku nigdy nie ma. */
function bar(): string {
  return renderToStaticMarkup(createElement(RunBar, { notes: [], onRun: noop, onFocusNote: noop }));
}

describe('an arrow that would close a circle refuses to land, and says nothing about it', () => {
  it('turns down the closing arrow and the self-loop, and still allows the diamond', () => {
    const doc = file();

    expect(
      isValidConnection({ source: 'c', target: 'a' }, doc),
      'c already comes after a, so this arrow would make work that never finishes',
    ).toBe(false);
    expect(
      isValidConnection({ source: 'a', target: 'a' }, doc),
      'a step cannot wait for itself either',
    ).toBe(false);
    expect(
      isValidConnection({ source: 'a', target: 'c' }, doc),
      'this one is a diamond, not a circle, and it has to be allowed. Without this line the ' +
        'two above also pass for a canvas on which no arrow can ever be drawn',
    ).toBe(true);
  });

  it('leaves the arrows exactly as they were when it turns one down', () => {
    const doc = file();

    expect(
      onConnect({ source: 'c', target: 'a' }, doc).links,
      'a refused arrow is not half-added and not queued — the file is the same file',
    ).toEqual(doc.links);
    expect(
      onConnect({ source: 'a', target: 'c' }, doc).links,
      'and the allowed one does land, or the line above is only measuring a canvas that ' +
        'accepts nothing',
    ).toEqual([...doc.links, { from: 'a', to: 'c' }]);
  });

  it('puts no message, no warning line and no live Run block on the screen', () => {
    const doc = file();
    onConnect({ source: 'c', target: 'a' }, doc);

    const html = bar();
    const run = buttonAttributes(html, 'Run');

    expect(run, 'the bar has to render a Run button; there is none').not.toBeNull();
    expect(
      /\bdisabled\b/.test(run ?? ''),
      'the user drew an arrow that did not land. That is not a mistake, so nothing about the ' +
        'workflow may stop being runnable because of it',
    ).toBe(false);
    expect(
      plain(html),
      'and there is no line above Run either: a circle that never happened is not something ' +
        'to fix. Reporting it would mean the user has to dismiss a message for doing nothing wrong',
    ).not.toContain('to fix');
  });
});
