/* PYTANIE STOI PRZY KROKU, KTÓRY JE ZADAŁ — i schodzi razem z biegiem.
 *
 * ZMIERZONA WADA, DWIE POŁOWY. Karta „Needs your answer" stała do dziś na dole KOLUMNY
 * STRUMIENIA, czyli po drugiej stronie ekranu od kafelka, który o coś zapytał. Przy czterech
 * krokach naraz nie było czym rozstrzygnąć, KTÓRY z nich stoi — podpis pod pytaniem jest
 * napisem, a kafelek świecący `attend` jest miejscem. Druga połowa: bieg, który zszedł
 * z nieodpowiedzianym pytaniem, ZOSTAWIAŁ kafelek zapalony na `attend`, bo lista agentów
 * liczy „czeka na ciebie" z wierszy historii, a te zostają na zawsze. Kafelek wołał więc
 * o decyzję w biegu, którego nie ma (niezmiennik 16 i 17 w jednym miejscu).
 *
 * DLACZEGO CAŁY EKRAN, A NIE SAM RYSUNEK. Karta wyrenderowana wprost przechodzi także wtedy,
 * gdy nikt jej nigdy nie montuje — niezmiennik 29 nazywa tę rodzinę po imieniu. Renderujemy
 * `<Run />` i pytamy JEGO markup, tą samą drogą co `../the-plan-reaches-the-screen.test.tsx`.
 *
 * DLACZEGO PLAN BEZ POZYCJI. To repo nie ma jsdom, a React Flow mierzy kafelki dopiero
 * w przeglądarce: pod `renderToStaticMarkup` oddaje ramę płótna z PUSTYMI pojemnikami. Droga,
 * po której człowiek widzi kafelki w tym środowisku, to lista kroków — ten sam kafelek, ten sam
 * kod, który stawia kartę pod nim.
 *
 * KOTWICE NIE MOGĄ BYĆ TEKSTEM PYTANIA. Zdanie, o które agent pyta, żyje na tym ekranie DWA
 * RAZY z założenia: raz w karcie z kontrolkami i raz jako wiersz historii, który zostaje na
 * zawsze, bo „że zapytał" naprawdę się wydarzyło. Punkt kotwiczony na samym zdaniu nie umie
 * odróżnić zdjętej karty od zdjętej historii, więc kotwicami są tu: znacznik karty, zachęta
 * pola odpowiedzi i napisy na przyciskach wyboru — trzy rzeczy, których wiersz historii nie ma.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { FeedLine, Step } from '../../../state/run';
import { useRun } from '../../../state/run';
import { line } from '../feed/fixtures/lines';
import { ANSWER_PROMPT } from '../feed/feed';
import { runFeed } from '../feed/live';
import Run from '../index';

const BUILD = 'Build';
const CHECK = 'Check';

/** Dwa kroki bez pozycji — plan, który okno rysuje listą, czyli jedyny czytelny tu markup. */
const STEPS: readonly Step[] = [
  { id: 's_build', name: BUILD, state: 'running' },
  { id: 's_check', name: CHECK, state: 'running' },
];

const WANTED = 'Should the reader keep a trailing comma at the end of a row?';
const OPTIONS = ['Keep it', 'Drop it'] as const;

const LINES: readonly FeedLine[] = [
  line.note(1, 0, BUILD, 'Rewriting the quote handling as a small state machine.'),
  line.note(2, 200, CHECK, 'Ran the checks — they did not work'),
  line.asked(3, 400, BUILD, WANTED, OPTIONS),
];

useRun.setState({
  workflow: 'Fix the CSV reader',
  steps: STEPS,
  links: null,
  lines: [...LINES],
});
runFeed.appendLines(LINES);
const standing = renderToStaticMarkup(<Run />);

/* Bieg schodzi tą samą drogą, którą chodzi produkt: `../io.ts` woła `view.runEnded()` w `finally`
 * każdego startu, więc także przy odmowie i przy Stopie. */
runFeed.runEnded();
const afterwards = renderToStaticMarkup(<Run />);

/* PYTANIE, KTÓREGO NIE DA SIĘ PRZYPISAĆ DO ŻADNEGO KROKU. Tak pyta lider i tak pyta pod-agent
 * rozpuszczony w trakcie biegu: podpis, którym nadaje, nie stoi w planie i nigdy nie stanie. */
const LEAD = 'Loadout';
const FROM_NOWHERE = 'Which folder should this go to?';
runFeed.appendLines([line.asked(9, 900, LEAD, FROM_NOWHERE, [])]);
const nobodysStep = renderToStaticMarkup(<Run />);

/** Markup jednego kafelka: od jego znacznika do znacznika następnego kafelka. */
function stepSlice(markup: string, id: string): string {
  const opens = markup.indexOf('data-step="' + id + '"');
  if (opens < 0) return '';
  const rest = markup.slice(opens);
  const next = rest.indexOf('data-step="', 1);
  return next < 0 ? rest : rest.slice(0, next);
}

/**
 * Jedna kolumna widoku pracy, wycięta z markupu — od jej znacznika do znacznika kolumny obok.
 *
 * Bez założenia, KTÓRA z nich stoi pierwsza: kolejność kolumn należy do układu i przyjeżdża
 * z makiety (`../run-matches-mockup.test.tsx`), a to kryterium jest o tym, w której kolumnie
 * stoi karta pytania.
 */
function columnOf(markup: string, marker: string): string {
  const opens = markup.indexOf(marker);
  if (opens < 0) return '';
  const rest = markup.slice(opens);
  const other = marker === 'data-plan-column' ? 'data-stream-column' : 'data-plan-column';
  const ends = rest.indexOf(other, 1);
  return ends < 0 ? rest : rest.slice(0, ends);
}

/** Ile razy ten napis stoi w markupie. */
function times(markup: string, what: string): number {
  return markup.split(what).length - 1;
}

/** Forma kafelka, który czeka na człowieka [`./tile.tsx`, tabela `TONE`]. */
const WAITS_FOR_YOU = 'border-attend-edge';

/** Znacznik karty pytania — jedyna rzecz, która odróżnia ją od wiersza historii. */
const CARD = 'data-asked';

describe('the question stands at the step that asked it', () => {
  it('runs on a stream that really holds a question nobody answered', () => {
    expect(
      runFeed.view.history.map((row) => row.kind),
      'the seeded stream has to carry the line where an agent asks, or every point below is ' +
        'about a screen with nothing to show',
    ).toContain('asked');
  });

  it('stands the card under the step whose worker asked, not at the foot of the stream', () => {
    const slice = stepSlice(standing, 's_build');
    expect(
      slice,
      'the run screen carries no card for the step ' +
        BUILD +
        ' at all, so asking what stands under it would be a question about nothing',
    ).not.toBe('');
    expect(
      slice,
      'the way to answer is not under the step that is waiting. It stood at the foot of the ' +
        'other column, and with four steps going at once the only thing saying which one waits ' +
        'was a name in small type. A place answers that question; a caption does not.',
    ).toContain(ANSWER_PROMPT);
    for (const option of OPTIONS) {
      expect(
        slice,
        'the choices the agent offered have to be under that same step: ' +
          option +
          ' is missing. Options come from the line, never from the view — a card that writes ' +
          'its own answers replies to something nobody asked.',
      ).toContain(option);
    }
  });

  it('leaves exactly one of them on the whole screen', () => {
    expect(
      times(standing, CARD),
      'there are ' +
        String(times(standing, CARD)) +
        ' ways to answer this one question on screen. The limit of live regions per fact is 1 ' +
        '[ARCHITECTURE §7]: two sets of buttons are two places a run can be let go, and the ' +
        'first drift between them is silent.',
    ).toBe(1);
  });

  it('lights the step that waits for a person, and only that one', () => {
    expect(
      stepSlice(standing, 's_build'),
      'the step that asked looks like every other one. Colour is the only thing that says ' +
        '"this one is waiting on you" from across the room [DESIGN §3].',
    ).toContain(WAITS_FOR_YOU);
    expect(
      stepSlice(standing, 's_check'),
      'the step that is simply working is lit as if it were waiting for a person. A colour ' +
        'that means everything means nothing.',
    ).not.toContain(WAITS_FOR_YOU);
  });

  it('takes the card away when the run goes away', () => {
    expect(
      times(afterwards, CARD),
      'the run is over and the card with its buttons is still standing. Pressing them reaches ' +
        'a worker that is not there: a control with no work left is worse than no control ' +
        '(invariant 16), and it is pinned to a relation the data no longer holds (invariant 17).',
    ).toBe(0);
    expect(afterwards, 'the field for typing an answer outlived the run too').not.toContain(
      ANSWER_PROMPT,
    );
  });

  it('stops lighting that step once the run is over', () => {
    expect(
      stepSlice(afterwards, 's_build'),
      'the run is over and the step still glows the colour that means "waiting for you". ' +
        'Nothing is waiting for anybody: the record that it once asked stays in the history, ' +
        'which is where a thing that happened belongs, but the live colour is a statement ' +
        'about now.',
    ).not.toContain(WAITS_FOR_YOU);
  });

  it('keeps a question nobody in the plan asked, where it has always stood', () => {
    expect(
      times(nobodysStep, CARD),
      'a question signed by a name the plan does not carry vanished from the screen entirely. ' +
        'That is how the lead asks, and how a worker dissolved mid-run asks: neither stands on ' +
        'a step, and neither ever will. A missing place to answer means the run stands on that ' +
        'question forever. No step means "we do not know who asked", never "nobody asked" ' +
        '(invariant 17).',
    ).toBe(1);
    /* 2026-08-31 — PYTAMY O KOLUMNĘ, NIE O POZYCJĘ W NAPISIE. Wersja porównująca dwa indeksy
       („karta stoi przed znacznikiem kolumny planu") mierzyła kolejność kolumn na ekranie,
       a nie miejsce karty: kiedy kolumna planu przeszła na lewo, punkt zaczął padać przy karcie
       stojącej dokładnie tam, gdzie ma stać. Wycinamy więc obie kolumny i pytamy każdą z nich
       osobno — to jest mocniejsze, bo żąda i obecności w tej właściwej, i nieobecności w tej
       drugiej. */
    expect(
      columnOf(nobodysStep, 'data-stream-column'),
      'it left the stream column. A question nobody in the plan asked has nowhere else to ' +
        'stand: the bottom of the stream is where it has always been.',
    ).toContain(CARD);
    expect(
      columnOf(nobodysStep, 'data-plan-column'),
      'it stood in the plan column, under a step that did not ask it. Putting it under the ' +
        'nearest card would be a relation invented by this screen.',
    ).not.toContain(CARD);
  });

  it('keeps the record of the question in the history, because asking really happened', () => {
    expect(
      afterwards,
      'the line where the agent asked was scrubbed from the transcript. What happened stays: ' +
        'the transcript of the run that just went down is the one thing a person comes back ' +
        'to this screen for.',
    ).toContain(WANTED);
  });
});
