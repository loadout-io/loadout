/* Kryterium 2 dla T-13, PRZEPISANE 2026-08-19 za zgodą właściciela: strzałka domykająca koło
 * ląduje, ale JAKO POWRÓT — z suficiem tur. Bez komunikatu, tak jak dotąd.
 *
 * CO SIĘ ZMIENIŁO I DLACZEGO. Pierwotne brzmienie tego pliku mówiło „koło jest UNIEMOŻLIWIONE,
 * nie zgłoszone", i było słuszne, dopóki koło znaczyło wyłącznie pracę, która się nie kończy.
 * Właściciel poprosił o kształt, którego bez powrotu nie da się wyrazić: implementer wysyła do
 * testera, tester zdaje raport, `fail` wraca do implementera, `pass` puszcza bieg dalej. Powrót
 * niesie `max_turns`, więc pętla bez końca dalej jest niewyrażalna — zmieniło się to, CO
 * odmawiamy, a nie to, przed czym bronimy. Projekt:
 * `docs/superpowers/specs/2026-08-19-petla-z-limitem-tur-design.md`.
 *
 * TRZY ODMOWY ZOSTAJĄ i są w tym pliku sądzone: pętla własna, ta sama strzałka drugi raz oraz
 * DRUGI powrót. Ostatnia jest nowa i jest w chwili gestu z premedytacją: Rust daje na drugi
 * powrót `Problem`, czyli po narysowaniu plik przestałby się zapisywać. Płótno, które pozwala
 * narysować rzecz blokującą zapis, kasuje pracę po cichu; płótno, które mówi „nie" od razu,
 * kosztuje jeden nieudany gest.
 *
 * SŁABA WERSJA tego kryterium to pojedyncze `expect(isValidConnection(...)).toBe(false)`.
 * Przechodzi dla `() => false`, czyli dla płótna, na którym nie da się narysować ani jednej
 * strzałki, i nikt tego nie zauważy przez tydzień, bo „nie da się połączyć" wygląda tak samo
 * jak „to połączenie jest złe". Rozróżniają to dwa przypadki, które MUSZĄ wrócić `true`: romb
 * (`a → c`) i właśnie powrót.
 *
 * DRUGA SŁABA WERSJA, nowa: sprawdzenie samego `true` dla krawędzi domykającej. Przechodzi dla
 * implementacji, która wpuszcza koło BEZ oznaczenia — a taki plik walidator Rusta odrzuca, więc
 * gest kończyłby się workflow, którego nie da się zapisać. Dlatego niżej sądzone jest, że
 * `onConnect` dokłada strzałkę Z LICZBĄ TUR.
 *
 * Trzecia część kryterium zostaje bez zmian: koło nie produkuje ani jednego zdania na ekranie.
 * Toast „cannot create cycle" był tu regresją i dalej nią jest.
 */
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { AgentStep, WorkflowFile } from '../../../state/workflows';
import { TURNS_BY_DEFAULT, isValidConnection, onConnect } from './connect';
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

describe('an arrow that would close a circle lands as a way back, and says nothing about it', () => {
  it('allows the closing arrow and the diamond, and still turns down the self-loop', () => {
    const doc = file();

    expect(
      isValidConnection({ source: 'c', target: 'a' }, doc),
      'c already comes after a, so this arrow closes a circle — and that is exactly the loop the ' +
        'owner asked for: the tester sends the work back to the implementer',
    ).toBe(true);
    expect(
      isValidConnection({ source: 'a', target: 'a' }, doc),
      'a step still cannot wait for itself: a way back to the same tile has no body to repeat, ' +
        'and nothing downstream knows what it would mean',
    ).toBe(false);
    expect(
      isValidConnection({ source: 'a', target: 'c' }, doc),
      'this one is a diamond, not a circle, and it has to be allowed. Without this line the ' +
        'refusal above also passes for a canvas on which no arrow can ever be drawn',
    ).toBe(true);
  });

  it('turns down a SECOND way back, in the gesture rather than at save time', () => {
    const doc = file();
    const withALoop = onConnect({ source: 'c', target: 'a' }, doc);

    expect(
      isValidConnection({ source: 'b', target: 'a' }, withALoop),
      'Rust refuses a file with two ways back, so drawing the second one would leave a workflow ' +
        'that cannot be saved. A canvas that allows the gesture loses work silently; one that ' +
        'says no costs a single failed drag.',
    ).toBe(false);
    expect(
      onConnect({ source: 'b', target: 'a' }, withALoop).links,
      'and the refused one is not half-added',
    ).toEqual(withALoop.links);
  });

  it('lands the closing arrow WITH a limit, and an ordinary one without', () => {
    const doc = file();

    expect(
      onConnect({ source: 'c', target: 'a' }, doc).links,
      'a way back without a limit is a file the validator refuses, so the gesture would end in a ' +
        'workflow that cannot be saved. The number is set in the same move as the arrow, because ' +
        'the document is not valid for even a moment without it.',
    ).toEqual([...doc.links, { from: 'c', to: 'a', max_turns: TURNS_BY_DEFAULT }]);
    expect(
      onConnect({ source: 'a', target: 'c' }, doc).links,
      'and an ordinary arrow stays ordinary: putting the key on every arrow would rewrite every ' +
        'workflow on disk and make each one a potential loop',
    ).toEqual([...doc.links, { from: 'a', to: 'c' }]);
  });

  it('leaves the arrows exactly as they were when it turns one down', () => {
    const doc = file();

    expect(
      onConnect({ source: 'a', target: 'a' }, doc).links,
      'a refused arrow is not half-added and not queued — the file is the same file',
    ).toEqual(doc.links);
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
