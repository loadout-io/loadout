/* SZUFLADA POD OBRAZEM: kafelek otwiera to, co powiedział TEN krok, w miejscu, w które
 * człowiek właśnie patrzy.
 *
 * PO CO. Do dziś jedyną drogą z obrazu planu do pracy jednego kroku był ekran, który ZAKRYWA
 * całe okno (`../session/`). Odpowiedź na pytanie „co robi ten kafelek" kosztowała więc utratę
 * z oczu wszystkich pozostałych — a bieg równoległy jest zwykłym biegiem (niezmiennik 11)
 * i patrzy się na niego w całości. DESIGN §7 wymienia wysuwany strumień kroku wprost, jako
 * jedną z powierzchni, które pojawiają się NAD tym, co już jest na ekranie.
 *
 * DLACZEGO CAŁY EKRAN, A NIE SAM KOMPONENT SZUFLADY. Szuflada wyrenderowana wprost przechodzi
 * także wtedy, gdy nikt jej nigdy nie montuje (niezmiennik 29) — a to jest dokładnie ta rodzina
 * wad, dla której powstało `../session/agent-screen-is-reachable.test.tsx`. Renderujemy `<Run />`
 * i pytamy JEGO markup.
 *
 * DLACZEGO WOŁAMY `openStepStream` WPROST. To repo nie ma jsdom, więc `onClick` nie odpali się
 * w żadnym kryterium tego rodzaju. Kryterium woła więc DOKŁADNIE tę funkcję, którą woła kafelek,
 * i pyta markup, czy zmienił się SKUTEK. Że prawdziwe kliknięcie i Escape naprawdę tam docierają,
 * dowodzi osobno `e2e/tests/step-stream-opens-and-closes.spec.ts` — prawdziwy chromium,
 * prawdziwa klawiatura.
 *
 * KOTWICA NIE MOŻE BYĆ ZDANIEM AGENTA. To samo zdanie stoi na tym ekranie w historii (kolumna
 * strumienia) i w szufladzie, więc punkt kotwiczony na samym zdaniu nie odróżnia jednego od
 * drugiego. Kotwicą jest znacznik szuflady i wycinek markupu, który do niej należy.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { FeedLine, Step } from '../../../state/run';
import { useRun } from '../../../state/run';
import { line } from '../feed/fixtures/lines';
import { runFeed } from '../feed/live';
import Run from '../index';
import { closeStepStream, openStepStream } from './opened';

const BUILD = 'Build';
const CHECK = 'Check';

const STEPS: readonly Step[] = [
  { id: 's_build', name: BUILD, state: 'running' },
  { id: 's_check', name: CHECK, state: 'running' },
];

const MINE = 'Rewriting the quote handling as a small state machine.';
const THEIRS = 'Ran the checks — they did not work';

const LINES: readonly FeedLine[] = [line.note(1, 0, BUILD, MINE), line.note(2, 200, CHECK, THEIRS)];

useRun.setState({
  workflow: 'Fix the CSV reader',
  steps: STEPS,
  links: null,
  lines: [...LINES],
});
runFeed.appendLines(LINES);

const shut = renderToStaticMarkup(<Run />);
openStepStream('s_build');
const open = renderToStaticMarkup(<Run />);
closeStepStream();
const shutAgain = renderToStaticMarkup(<Run />);

/** Znacznik szuflady. Niesie klucz kroku, żeby dało się powiedzieć, że otworzył się TEN. */
const DRAWER = 'data-step-stream';

/** Druga kolumna widoku pracy — ta, w której stoi obraz planu. */
const PLAN_COLUMN = 'data-plan-column';

/** Lista kroków, czyli to, co obraz rysuje, kiedy plik nie mówi, gdzie kafelki stoją. */
const PICTURE = 'data-step-list';

/**
 * Sama szuflada, wycięta z markupu. Pusta, kiedy jej nie ma.
 *
 * 2026-08-31 — WYCINEK KOŃCZY SIĘ NA KOŃCU SWOJEJ KOLUMNY, nie na końcu ekranu. Wersja biorąca
 * wszystko do końca dokumentu przechodziła wyłącznie dlatego, że kolumna planu stała na ekranie
 * OSTATNIA: kiedy kolumny zamieniły się miejscami, do „szuflady" wpadł cały strumień razem
 * z wierszami cudzych kroków, i punkt o zawężeniu do jednego kroku sądził pół ekranu. Granicą
 * jest znacznik następnej kolumny — po którejkolwiek stronie ona stoi.
 */
function drawerIn(markup: string): string {
  const at = markup.indexOf(DRAWER);
  if (at < 0) return '';
  const rest = markup.slice(at);
  const ends = [rest.indexOf('data-stream-column', 1), rest.indexOf(PLAN_COLUMN, 1)].filter(
    (one) => one > 0,
  );
  return ends.length === 0 ? rest : rest.slice(0, Math.min(...ends));
}

/** Tekst, który człowiek naprawdę czyta — bez znaczników, więc bez klas i atrybutów `data-*`. */
function visible(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('a step opens its own stream under the picture of the plan', () => {
  it('runs on a screen that really draws the two steps', () => {
    for (const step of STEPS) {
      expect(
        shut,
        'the run screen carries nothing for ' +
          step.name +
          ', so every point below would be about a picture with nothing in it',
      ).toContain('data-step="' + step.id + '"');
    }
  });

  it('shows nothing of the kind until somebody opens one', () => {
    expect(
      drawerIn(shut),
      'the panel stood open with nobody having asked for it, so "opening it works" below would ' +
        'be true of something that is simply always there',
    ).toBe('');
  });

  it('opens the one belonging to the step that was pressed', () => {
    expect(
      drawerIn(open),
      'pressing a card changed nothing. A control whose handler has no effect is worse than no ' +
        'control at all (invariant 16).',
    ).not.toBe('');
    expect(open, 'and it is the panel of THAT step').toContain(DRAWER + '="s_build"');
  });

  it('stands in the plan column, below the picture and never beside it', () => {
    const column = open.indexOf(PLAN_COLUMN);
    const picture = open.indexOf(PICTURE);
    const drawer = open.indexOf(DRAWER);
    expect(column, 'the work view has no plan column at all').toBeGreaterThan(-1);
    expect(picture, 'the plan column draws no picture at all').toBeGreaterThan(-1);
    expect(
      drawer,
      'the panel opened outside the column that holds the picture. A third column is a third ' +
        'axis on a screen that ARCHITECTURE §7 allows two, and they have to be perpendicular.',
    ).toBeGreaterThan(column);
    expect(
      drawer,
      'the panel opened above the picture and pushed it down. What a person pressed has to stay ' +
        'where it was: a picture that jumps under the pointer answers a question nobody asked.',
    ).toBeGreaterThan(picture);
  });

  it('carries what that step said, and nothing anybody else said', () => {
    const words = visible(drawerIn(open));
    expect(
      words,
      'what ' +
        BUILD +
        ' said is missing from its own panel. The view of one step is the same stream with a ' +
        'filter, and an empty one answers nothing.',
    ).toContain(MINE);
    expect(
      words,
      'a line belonging to ' +
        CHECK +
        ' reached the panel of ' +
        BUILD +
        '. A panel that opens without narrowing to one step is the same panel for every card, ' +
        'and with two running the person reads one and looks at the other.',
    ).not.toContain(THEIRS);
  });

  it('slides in once, on the spring the sheet keeps for things that appear', () => {
    expect(
      drawerIn(open).slice(0, drawerIn(open).indexOf('>')),
      'the panel appears in a jump. An element that appears in a jump reads as the view itself ' +
        'jumping — the eye cannot tell whether it is looking at the same place [DESIGN §7]. The ' +
        'sheet carries this as one class, and it fires once on arrival.',
    ).toContain('enter');
  });

  it('has a way out that a person can point at', () => {
    const words = visible(drawerIn(open));
    expect(
      words,
      'a panel that covers half the column and cannot be shut is a trap, not a panel',
    ).toContain('Close');
    expect(
      drawerIn(shutAgain),
      'taking the way out left the panel standing. The way out has to have the same effect as ' +
        'the card, in reverse — one field, one truth.',
    ).toBe('');
    expect(
      shutAgain,
      'and the picture of the plan is still there, unchanged by the trip',
    ).toContain('data-step="s_build"');
  });
});
