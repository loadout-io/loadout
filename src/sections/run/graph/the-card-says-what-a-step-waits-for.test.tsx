/* Karta kroku mowi, NA CO ten krok czeka — imieniem poprzednika, nie polozeniem w kolumnie.
 *
 * ── SKAD TEN PLIK, W TEJ POSTACI ───────────────────────────────────────────────────────────
 *
 * Do 2026-09-01 odpowiadala na to RYNNA po lewej: numer poziomu w kolku i kreska miedzy
 * poziomami. Wlasciciel kazal ja zdjac w dwoch krokach. Najpierw numer („numeracja bez sensu,
 * moze jej wgl nie powinno byc") — bo poziom mowil prawde o strzalkach, ale kolejnosc krokow
 * w pliku nie musi byc topologiczna i na jego wlasnym planie kolumna czytala sie
 * `1 2 2 6 4 2 3 3 3`. Potem cala rynne („calkowiecie to wywal") — bo po zdjeciu numeru zostaly
 * puste obrecze i kreski, czyli KSZTALT mowiacy to, co karta obok mowi SLOWEM.
 *
 * PYTANIE ZOSTAJE, ZMIENIL SIE NOSNIK ODPOWIEDZI. Ekran dalej nie ma prawa obiecac kolejnosci,
 * ktorej w pliku nie ma — i dalej musi pokazac, na co krok czeka. Robi to dzis zdanie
 * `after <krok>` na karcie, i robi to LEPIEJ niz kreska: nazywa poprzednika po IMIENIU, wiec
 * dwa kroki wiszace na tym samym kroku mowia to samo imie i widac to bez liczenia pozycji.
 *
 * DLATEGO TE PUNKTY SADZA MARKUP KARTY, a nie ksztalt obok niej. Fikstura jest naprawde
 * rozgalezniona (dwa kroki po jednym poprzedniku, potem zejscie w jeden) — na lancuchu KAZDA
 * odpowiedz wyglada dobrze i punkt bylby zielony o niczym.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Link } from '../../../state/workflows';
import { RunGraph } from './graph';
import type { GraphStep, Plan } from './model';

/* CZTERY KROKI, DWA NA JEDNYM POZIOMIE — najmniejszy plan, na którym pozycja w tablicy i poziom
 * w grafie dają RÓŻNE liczby. Wszystkie czekają, bo numer stoi w kółku wyłącznie przy kroku,
 * który się jeszcze niczym nie skończył: krok zrobiony nosi ptaszka, a pracujący kropkę. */
const STEPS: readonly GraphStep[] = [
  { id: 's_plan', name: 'Plan the work', status: 'waiting' },
  { id: 's_read', name: 'Read the code', status: 'waiting' },
  { id: 's_ask', name: 'Ask the owner', status: 'waiting' },
  { id: 's_write', name: 'Write the plan', status: 'waiting' },
];

/** Dwie strzałki z jednego kroku i zejście obu w jeden. Poziomy: 1, 2, 2, 3. */
const LINKS: readonly Link[] = [
  { from: 's_plan', to: 's_read' },
  { from: 's_plan', to: 's_ask' },
  { from: 's_read', to: 's_write' },
  { from: 's_ask', to: 's_write' },
];

const BRANCHED: Plan = { steps: STEPS, links: LINKS };

/* TEN SAM PLAN Z DROGĄ POWROTNĄ. `max_turns` znaczy „spróbuj jeszcze raz", a nie „potem":
 * strzałka wraca do kroku, który już był, i domyka koło z rozmysłu. Policzona jako kolejność
 * daje graf z cyklem, w którym poziom przestaje istnieć — i dlatego przy liczeniu poziomów
 * musi wypaść. Tak samo czyta ją Rust: `workflow::check` liczy koło na strzałkach BEZ powrotów,
 * a `workflow::unroll` rozwija pętlę na literalne rundy, zanim planista zobaczy graf. */
const WITH_A_WAY_BACK: Plan = {
  steps: STEPS,
  links: [...LINKS, { from: 's_write', to: 's_plan', max_turns: 3 }],
};

/* KOLEJNOŚĆ W PLIKU NIE JEST KOLEJNOŚCIĄ PRACY, i to nie jest przypadek wymyślony na potrzeby
 * punktu. Na zrzucie właściciela krok „Backend" stoi piąty i czeka na krok „Architect", którego
 * lista pokazuje DALEJ — plik workflow trzyma kroki w kolejności, w jakiej człowiek je dorzucał,
 * a ta z zależnościami nie ma nic wspólnego. Cztery kroki niżej stoją więc dokładnie odwrotnie,
 * niż idą. */
const LISTED_BACKWARDS: Plan = {
  steps: [
    { id: 's_fourth', name: 'Ship it', status: 'waiting' },
    { id: 's_third', name: 'Check it', status: 'waiting' },
    { id: 's_second', name: 'Build it', status: 'waiting' },
    { id: 's_first', name: 'Draw it', status: 'waiting' },
  ],
  links: [
    { from: 's_first', to: 's_second' },
    { from: 's_second', to: 's_third' },
    { from: 's_third', to: 's_fourth' },
  ],
};

const DRAWN = renderToStaticMarkup(<RunGraph plan={BRANCHED} />);
const DRAWN_WITH_A_WAY_BACK = renderToStaticMarkup(<RunGraph plan={WITH_A_WAY_BACK} />);
const DRAWN_BACKWARDS = renderToStaticMarkup(<RunGraph plan={LISTED_BACKWARDS} />);

/** Wycinek karty tego kroku — od jej znacznika do znacznika nastepnej. */
function cardOf(markup: string, id: string): string {
  const opens = markup.indexOf('data-step="' + id + '"');
  if (opens < 0) return '';
  const rest = markup.slice(opens);
  const next = rest.indexOf('data-step="', 1);
  return next < 0 ? rest : rest.slice(0, next);
}

/** Czysty tekst tej karty, bez znacznikow. */
function saysOn(markup: string, id: string): string {
  return cardOf(markup, id)
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('the card of a step says what it waits for, by name', () => {
  it('names the step it waits for, and names it the same for two steps that wait for one', () => {
    expect(
      saysOn(DRAWN, 's_read'),
      'the second step of this plan drew no card at all, so both comparisons below would run ' +
        'against two empty strings and pass on nothing.',
    ).not.toBe('');

    expect(
      saysOn(DRAWN, 's_read'),
      '"Read the code" waits for "Plan the work" and the card does not say so. Until today a ' +
        'gutter of dots and lines said it with a shape; the shape is gone, so the sentence on ' +
        'the card is the only place left that answers "what has to happen before this".',
    ).toContain('Plan the work');

    expect(
      saysOn(DRAWN, 's_ask'),
      '"Read the code" and "Ask the owner" both wait for "Plan the work" and for nothing else, ' +
        'so neither comes before the other. Their cards have to name the same step: naming ' +
        'different ones, or naming each other, is an order the file does not have (invariant 17).',
    ).toContain('Plan the work');
  });

  it('names the arrows even when the file lists a step after the steps waiting for it', () => {
    expect(
      saysOn(DRAWN_BACKWARDS, 's_second'),
      'these four steps run one after another and the file lists them in reverse — which is what ' +
        'a file looks like when a person kept adding steps and wiring them up afterwards. The ' +
        'card has to read the arrow, not the line above it in the list: the run that prompted ' +
        'this had "Backend" waiting for a step written below it.',
    ).toContain('Draw it');
  });

  it('says nothing about an order when a step waits for nothing', () => {
    expect(
      saysOn(DRAWN, 's_plan'),
      'the step nothing comes before is described as waiting for something. Inventing a ' +
        'predecessor for the first step is the same lie the numbers told, moved into a sentence.',
    ).not.toContain('after ');
  });

  it('does not turn a way back into an order', () => {
    expect(
      saysOn(DRAWN_WITH_A_WAY_BACK, 's_read'),
      'an arrow that only sends the run BACK to try again is being read as "this comes after ' +
        'that". A way back is a repeat, not a place in the order — Rust drops those links before ' +
        'it checks the plan for circles (`workflow/check.rs`), and the screen has to agree.',
    ).toContain('Plan the work');
  });
});
