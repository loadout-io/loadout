/* Lista uwag pokazuje kilka, a resztę na żądanie — i te kilka to te, które blokują Run.
 *
 * 2026-08-23 — ZGŁOSZENIE WŁAŚCICIELA ZE ZRZUTU: „ogarnij ten UI z errorami bo mi zalewa ekran,
 * powinnien miec kilka pokazanych i ewentualnie reszte na toggle". Pasek rysował KAŻDĄ uwagę,
 * a reguła o dwóch krokach bez strzałki w jednym folderze zgłasza się PER PARĘ — więc dziesięć
 * nienazwanych kafelków daje czterdzieści pięć zdań, płótna spod nich nie widać, a przycisk Run
 * stoi na dole listy.
 *
 * SŁABĄ WERSJĄ jest policzenie samych widocznych wierszy. Przechodzi ją implementacja, która po
 * prostu OBCINA listę — a wtedy uwagi, której nie widać, nikt nigdy nie naprawi, bo nie ma jak
 * jej odsłonić. Dlatego drugie kryterium wymaga, żeby reszta była o jedno kliknięcie dalej,
 * i żeby po tym kliknięciu były WSZYSTKIE.
 *
 * TRZECIE jest tym, którego brak czyni z tego regresję: kiedy widać trzy z czterdziestu, to muszą
 * być te trzy, które ZATRZYMUJĄ BIEG. Lista w kolejności walidatora pokazywałaby czasem trzy
 * ostrzeżenia i chowała pod przyciskiem jedyny problem, przez który nic nie rusza.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Note } from '../../../state/workflows';
import { RunBar } from './problems';

const noop = (): void => undefined;

/** Ostrzeżenie — nie blokuje Run. Tekst niesie numer, żeby dało się je od siebie odróżnić. */
function warning(at: number): Note {
  return {
    level: 'warning',
    stepId: `s_${String(at)}`,
    message: `"New step" and "New step" can run at the same time (${String(at)})`,
  };
}

/** Problem — blokuje Run. Jeden, postawiony NA KOŃCU listy, bo o to w kryterium chodzi. */
const BLOCKER: Note = {
  level: 'problem',
  stepId: 's_last',
  message: '"Design" does not say what to do, so the agent would have to guess.',
};

const FLOOD: Note[] = [...Array.from({ length: 12 }, (_, at) => warning(at)), BLOCKER];

/** Markup tak, jak czyta go człowiek: React zapisuje cudzysłowy jako encje. */
function bar(notes: Note[]): string {
  return renderToStaticMarkup(<RunBar notes={notes} onRun={noop} onFocusNote={noop} />)
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'");
}

/** Ile zdań uwag stoi w markupie. Liczone po kropce wagi, bo ona jest w każdym wierszu raz. */
function rows(markup: string): number {
  return markup.split('●').length - 1;
}

describe('a wall of validator notes does not take the screen', () => {
  it('shows a few of them, not all thirteen', () => {
    const shown = rows(bar(FLOOD));

    expect(
      shown,
      'every note was drawn. One validator rule reports per PAIR of steps, so a dozen unnamed ' +
        'tiles bury the canvas and push Run off the bottom of the panel.',
    ).toBeLessThan(FLOOD.length);
    expect(
      shown,
      'and at least something, or the panel says a count and shows nothing',
    ).toBeGreaterThan(0);
  });

  it('says how many it is holding back, and offers them in one press', () => {
    const markup = bar(FLOOD);

    expect(
      markup,
      'a list that is merely truncated hides notes nobody can ever reach — which is worse than ' +
        'the flood, because the count says there is something and the screen will not show it.',
    ).toContain('data-show-all-notes');
    expect(
      markup,
      'the number belongs on the button: "Show all" does not say what you are agreeing to, and ' +
        'at forty notes that is the difference between a glance and a page of text.',
    ).toContain(`Show ${String(FLOOD.length - rows(markup))} more`);
  });

  it('shows the ones that block Run first, whatever order the validator sent', () => {
    const markup = bar(FLOOD);

    expect(
      markup,
      'the one problem in this list arrived LAST from the validator, and it is the only thing ' +
        'stopping the run. Shown in arrival order it would sit under the button, behind twelve ' +
        'warnings that let the run start.',
    ).toContain(BLOCKER.message);
  });

  it('draws no toggle when everything already fits', () => {
    const markup = bar([BLOCKER]);

    expect(
      markup.includes('data-show-all-notes'),
      'a control that reveals nothing is a control without an effect (invariant 16)',
    ).toBe(false);
    expect(markup, 'and the one note is right there').toContain(BLOCKER.message);
  });
});
