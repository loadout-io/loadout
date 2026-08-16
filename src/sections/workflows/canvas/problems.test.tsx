/* Kryterium 6 dla T-13: problem blokuje Run i mówi dlaczego; ostrzeżenie nie blokuje.
 *
 * Słaba wersja tego kryterium to `expect(html).toContain('2 things to fix')`. Przechodzi dla
 * napisu policzonego z `notes.length` przy zawsze wyłączonym Run — a wtedy ostrzeżenie
 * o niepodłączonym kroku blokuje uruchomienie i użytkownik nie wie dlaczego. Rozróżnia to
 * przypadek B, w którym jest sama uwaga wagi `warning`, a Run zostaje ŻYWY.
 *
 * Druga rzecz, która musi być asercją, a nie literałem: podpowiedź zablokowanego przycisku jest
 * porównywana z `notes[0].message`, nie z tekstem wpisanym w tym pliku. „Fix the errors first"
 * pod zablokowanym przyciskiem to przycisk bez wyjaśnienia — użytkownik widzi, że nie może
 * kliknąć, i nie wie, czego szukać [T3 §5.3].
 *
 * Uwagi przychodzą z walidatora Rusta (T-12) i są tu podane wprost. Frontend ich nie wymyśla:
 * gdyby liczył je po swojemu, mielibyśmy dwa źródła tego samego zdania, a jedno z nich zawsze
 * jest nieaktualne.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Note } from '../../../state/workflows';
import type { NoteFocus } from './problems';
import { RunBar, focusNote } from './problems';

/** Zdanie z `workflow::check`, słowo w słowo. Ono ląduje na ekranie i w podpowiedzi Run. */
const CIRCLE = 'These steps point back at each other in a circle. Work would never finish.';
const LONELY = '"Check" is not connected to the rest of the workflow.';

function circle(): Note {
  return { level: 'problem', stepId: 's2', message: CIRCLE };
}

function lonely(): Note {
  return { level: 'warning', stepId: 's_check', message: LONELY };
}

function noop(): void {
  /* sterowany pasek: w statycznym renderze nic tego nie woła */
}

function markup(notes: Note[]): string {
  return renderToStaticMarkup(<RunBar notes={notes} onRun={noop} onFocusNote={noop} />);
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

function buttonAttributes(html: string, label: string): string {
  for (const hit of html.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)) {
    if (plain(hit[2] ?? '') === label) return hit[1] ?? '';
  }
  throw new Error('the bar renders no button labelled ' + label);
}

/** Znak, którym React opakowuje wartości atrybutów, zbudowany z kodu zamiast wpisany.
 *
 * Powód jest zmierzony, nie estetyczny (2026-08-16): `checks/quick-vocabulary.sh` paruje znaki
 * cytowania po kolei i nie zna literałów wyrażeń regularnych. Wpisany wprost w wyrażenie niżej
 * rozjeżdżał parowanie do końca pliku i zgłaszał trafienie kilkanaście linii dalej — na kluczu
 * `nodes` z `FitViewOptions`, który nie jest tekstem dla użytkownika i nie ma jak nim zostać. */
const MARK = String.fromCharCode(34);

/** Wartość atrybutu `title`, albo `null`, kiedy przycisk go nie niesie. */
function titleOf(attributes: string): string | null {
  const opens = ' title=' + MARK;
  const start = attributes.indexOf(opens);
  if (start < 0) return null;

  const rest = attributes.slice(start + opens.length);
  const closes = rest.indexOf(MARK);
  return closes < 0 ? null : plain(rest.slice(0, closes));
}

/** Zapisywacz wywołań. Ręczny, bo pytanie brzmi „z czym dokładnie", a nie „czy w ogóle". */
function recorder<A extends unknown[]>(): { calls: A[]; fn: (...args: A) => void } {
  const calls: A[] = [];
  return {
    calls,
    fn: (...args: A) => {
      calls.push(args);
    },
  };
}

function first(notes: Note[]): Note {
  const hit = notes[0];
  if (hit === undefined) throw new Error('this test needs at least one note');
  return hit;
}

describe('a problem stops Run and says which one; a warning stops nothing', () => {
  it('counts both notes, blocks Run, and puts the first problem in the tooltip word for word', () => {
    const notes = [circle(), lonely()];
    const html = markup(notes);

    expect(
      plain(html),
      'one line above the button, both notes counted. Two lines about two notes is two ' +
        'places where the same fact lives',
    ).toContain('2 things to fix');

    const run = buttonAttributes(html, 'Run');
    expect(
      /\bdisabled\b/.test(run),
      'a problem means this workflow would not finish, so it does not start',
    ).toBe(true);
    expect(
      titleOf(run),
      'the tooltip is the note itself, straight from the checker. Anything written here ' +
        'instead is a second copy of the same sentence, and it goes stale the day the ' +
        'checker changes its wording',
    ).toBe(first(notes).message);
    expect(
      titleOf(run),
      'and it is never a sentence that tells the user nothing about what to look for',
    ).not.toBe('Fix the errors first');
  });

  it('leaves Run alive when the only note is a warning, and counts one thing in the singular', () => {
    const html = markup([lonely()]);

    expect(plain(html), 'one note, one thing — not "1 things"').toContain('1 thing to fix');

    const run = buttonAttributes(html, 'Run');
    expect(
      /\bdisabled\b/.test(run),
      'a step nobody wired up is worth saying out loud and is not worth refusing to run over. ' +
        'A bar that blocks on every note turns this into a lock with no key',
    ).toBe(false);
  });

  it('moves the canvas onto the step a note names and opens its panel', () => {
    const fitView = recorder<[Parameters<NoteFocus['fitView']>[0]]>();
    const openPanel = recorder<[string]>();

    focusNote(circle(), { fitView: fitView.fn, openPanel: openPanel.fn });

    expect(
      fitView.calls,
      'exactly one move, with the step the note names, the one duration in the system and a ' +
        'ceiling on the zoom so a lone tile does not fill the screen',
    ).toEqual([[{ nodes: [{ id: 's2' }], duration: 400, maxZoom: 1.2 }]]);
    expect(
      openPanel.calls,
      'and the panel of that same step opens, so the field to fix is already in front of you',
    ).toEqual([['s2']]);
  });

  it('does nothing at all for a note about the whole file, because there is nothing to move to', () => {
    const fitView = recorder<[Parameters<NoteFocus['fitView']>[0]]>();
    const openPanel = recorder<[string]>();

    focusNote(
      { level: 'problem', stepId: null, message: 'There are no steps yet.' },
      { fitView: fitView.fn, openPanel: openPanel.fn },
    );

    expect(fitView.calls, 'there is no tile to centre on').toEqual([]);
    expect(openPanel.calls, 'and no panel to open').toEqual([]);
  });
});
