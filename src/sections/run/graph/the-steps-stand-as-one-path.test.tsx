/* Kolumna planu jest JEDNĄ ŚCIEŻKĄ, a nie stosem pudełek.
 *
 * ZMIERZONA WADA (2026-08-31, zgłoszenie właściciela: „UX totalnie nieoczywisty"). Kroki biegu
 * stały w kolumnie jeden pod drugim, każdy w osobnej karcie, i ani jeden piksel nie mówił, że
 * są ciągiem. Człowiek patrzący na cztery karty nie widział ani ILE ich zostało, ani KTÓRE ma
 * za sobą — obie te rzeczy musiał policzyć sam, za każdym razem. Makieta
 * (`docs/mockup/index.html`, reguły `.rail`/`.step`/`.pip`) odpowiada na to ścieżką: znacznik
 * przy każdym kroku i linia, która je łączy.
 *
 * DLACZEGO PRZEZ `RunGraph`, A NIE PRZEZ SAM ZNACZNIK. Znacznik wyrenderowany wprost przechodzi
 * także wtedy, gdy nic go nigdy nie montuje — dokładnie ta cicha porażka, którą niezmiennik 29
 * nazywa po imieniu. Plan bez pozycji renderuje ścieżkę i to jest droga, po której człowiek te
 * znaczniki naprawdę widzi (to samo rozstrzygnięcie i ten sam powód, co w
 * `./each-state-draws-its-own-step.test.tsx`).
 *
 * DLACZEGO KSZTAŁT LINII CZYTAMY Z KLAS, A NIE Z ARKUSZA. Klasa jest tym, co dojeżdża do
 * przeglądarki razem z elementem; reguła w pliku `.css` sprawdzona osobno przechodzi także
 * wtedy, gdy żaden element jej nie nosi. Linia ciągła i linia przerywana różnią się tu więc
 * tak, jak różnią się na ekranie: zestawem klas na tym jednym elemencie.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Step } from '../../../state/run';
import { useRun } from '../../../state/run';
import type { Link } from '../../../state/workflows';
import Run from '../index';
import { RunGraph } from './graph';
import type { GraphStep, Plan } from './model';

/* CZTERY KROKI, DWA ZA SOBĄ — dokładnie ten stan, który rysuje makieta: dwa zrobione, jeden
 * w pracy, jeden czekający. Bez pozycji, bo to jest droga, na której widać kafelki w środowisku
 * bez przeglądarki. */
const FOUR: readonly GraphStep[] = [
  { id: 's1', name: 'Reproduce', status: 'done' },
  { id: 's2', name: 'Fix', status: 'done' },
  { id: 's3', name: 'Tests pass', status: 'working' },
  { id: 's4', name: 'Second opinion', status: 'waiting' },
];

/* STRZAŁKI DOSZŁY 2026-08-31 I SĄ TU WARUNKIEM SENSU, nie ozdobą planu.
 *
 * Punkty niżej mówią o DŁUGOŚCI drogi: czwarty krok nosi „4", a linia biegnie od każdego kroku
 * do następnego. Oba te zdania są prawdziwe wyłącznie wtedy, gdy te cztery kroki naprawdę idą
 * jeden po drugim — a do dziś plan pod nimi nie miał ani jednej strzałki, czyli mówił coś
 * dokładnie odwrotnego: cztery kroki, na które nic nie czeka, wolno puścić RAZEM i bieg tak
 * właśnie robi (`engine::scheduler` wypuszcza w pierwszym obrocie wszystkie kroki o zerowym
 * stopniu wejściowym). Numer liczył się wtedy z pozycji w tablicy i przypadkiem zgadzał się
 * z tym, czego te punkty żądają; od chwili, w której liczy się ze strzałek (`./model.ts`,
 * `levelsOf`, i `./the-number-says-what-a-step-waits-for.test.tsx`), plan musi te strzałki
 * mieć — inaczej te punkty żądałyby od ekranu, żeby skłamał. */
const IN_A_ROW: readonly Link[] = [
  { from: 's1', to: 's2' },
  { from: 's2', to: 's3' },
  { from: 's3', to: 's4' },
];

const PLAN: Plan = { steps: FOUR, links: IN_A_ROW };
const DRAWN = renderToStaticMarkup(<RunGraph plan={PLAN} />);

/** Czysty tekst tego wycinka, bez znacznikow i bez podwojnych spacji. */
function textOf(piece: string): string {
  return piece
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Wycinek karty tego kroku — od jej znacznika do znacznika nastepnej. */
function cardOf(markup: string, id: string): string {
  const opens = markup.indexOf('data-step="' + id + '"');
  if (opens < 0) return '';
  const rest = markup.slice(opens);
  const next = rest.indexOf('data-step="', 1);
  return next < 0 ? rest : rest.slice(0, next);
}

/** Czysty tekst tej karty. */
function saysOn(markup: string, id: string): string {
  return cardOf(markup, id)
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/* 2026-09-01 — DWA PUNKTY TEGO BLOKU ZNIKLY RAZEM ZE SWOIM PRZEDMIOTEM, i mowie to wprost
 * zamiast zostawiac je zielone nad niczym. Pytaly o KRESKE miedzy krokami: czy laczy je, czy nie
 * wisi po ostatnim, czy za zrobionym jest ciagla. Wlasciciel kazal zdjac cala rynne znacznikow
 * („calkowiecie to wywal"), bo po wczesniejszym zdjeciu numeru zostaly w niej puste obrecze
 * i kreski — ksztalt mowiacy to, co karta obok mowi slowem.
 *
 * Fakt, ktorego tamte punkty pilnowaly — „widac, ktory krok jest za toba" — nie zniknal razem
 * z nimi: sadzi go punkt nizej, na chipie stanu, czyli na tym, co czyta czlowiek. Drugi fakt,
 * „co po czym", przeszedl na zdanie `after <krok>` i ma wlasny plik
 * (`the-card-says-what-a-step-waits-for.test.tsx`), gdzie jest sadzony po IMIENIU poprzednika,
 * a nie po polozeniu w kolumnie. */
describe('cztery kroki stoją jako jedna ścieżka', () => {
  it('gives every step of the run its own place on the path', () => {
    for (const step of FOUR) {
      expect(
        saysOn(DRAWN, step.id),
        'the step "' +
          step.name +
          '" is in the plan and has no card of its own on the path. A plan that draws some of ' +
          'its steps and drops the rest is worse than one that draws none: a person reads what ' +
          'is there and takes it for the whole run.',
      ).toContain(step.name);
    }
  });

  it('tells a step that is behind you from one nobody has started', () => {
    expect(
      saysOn(DRAWN, 's1'),
      'the finished step says nothing about being finished, so nothing on the path says it is ' +
        'behind you.',
    ).not.toBe('');
    expect(
      saysOn(DRAWN, 's4').replace(FOUR[3]?.name ?? '', ''),
      'a step nobody has started reads the same as the finished one, so the column says the ' +
        'same thing about work that is done and work that has not begun.',
    ).not.toBe(saysOn(DRAWN, 's1').replace(FOUR[0]?.name ?? '', ''));
  });
});

const STEPS: readonly Step[] = [
  { id: 's_build', name: 'Fix', state: 'running' },
  { id: 's_read', name: 'Second opinion', state: 'pending' },
];
const LINKS: readonly Link[] = [{ from: 's_build', to: 's_read' }];

useRun.setState({ workflow: 'Ship a feature', steps: STEPS, links: LINKS });
const SCREEN = renderToStaticMarkup(<Run />);

useRun.setState({
  steps: [
    { id: 's_build', name: 'Fix', state: 'running' },
    { id: 's_ship', name: 'Open the change', state: 'pending' },
  ],
  links: [{ from: 's_build', to: 's_ship' }],
});
const OTHER = renderToStaticMarkup(<Run />);

/** Zdanie o tym, co się stanie po ostatnim kroku — od jego znacznika do końca kolumny. */
function afterRunIn(markup: string): string {
  const opens = markup.indexOf('data-after-run');
  return opens < 0 ? '' : textOf(markup.slice(opens));
}

describe('kolumna mówi, co się stanie, zanim się stanie', () => {
  it('says what happens when the last step turns green, before it happens', () => {
    expect(
      afterRunIn(SCREEN),
      'the run screen never says what happens when the last step turns green. A person watching ' +
        'four agents work has no way to find out where this ends except by waiting for it — and ' +
        'that is the question they are sitting in front of the screen with.',
    ).not.toBe('');
  });

  it('names that last step from the plan, never from a sentence written into the screen', () => {
    expect(
      afterRunIn(SCREEN),
      'the sentence does not name the step this run ends on, so it says "something happens at ' +
        'the end" — which is what the person already knew',
    ).toContain('Second opinion');
    expect(
      afterRunIn(OTHER),
      'a second run ending on a differently named step gets the same sentence, so the name is ' +
        'written into the screen rather than read off the plan. A screen that names a step this ' +
        'run does not have is worse than one that names none (invariant 17).',
    ).toContain('Open the change');
  });

  it('says the run stops there, and that the work goes nowhere without a person', () => {
    const said = afterRunIn(SCREEN);
    expect(
      said,
      'nothing says the run STOPS at the last step. Without it the sentence is a label, not an ' +
        'answer: agents that keep going on their own are the one thing a person watching this ' +
        'screen is afraid of.',
    ).toMatch(/stops/);
    expect(
      said,
      'nothing says where the work ends up. Loadout commits every step onto its own branch and ' +
        'pushes nothing (`commands::isolate::finish`, `Kept::OnABranch`) — that promise is the ' +
        'reason a person dares press Start, and it is on the screen nowhere.',
    ).toMatch(/pushe[sd]|branch/);
  });

  it('stays off a screen with no plan, where it would be a promise about nothing', () => {
    useRun.setState({ steps: [], links: null });
    expect(
      afterRunIn(renderToStaticMarkup(<Run />)),
      'the screen promises what happens after a run it knows no steps of. A sentence about the ' +
        'end of a plan nobody has is a relation that is not in the data (invariant 17).',
    ).toBe('');
  });
});
