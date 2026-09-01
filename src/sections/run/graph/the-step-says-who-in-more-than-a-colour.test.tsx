/* Wiersz „kto robi ten krok" na karcie kroku — sądzony na tej samej drodze, którą widzi człowiek.
 *
 * WYROCZNIĄ JEST MAKIETA, reguły `.sbox .who` i `.face` w `docs/mockup/index.html`:
 *
 *     .sbox .who{display:flex;align-items:center;gap:8px;margin-top:8px;
 *       font-family:var(--ui);font-size:12px;color:var(--body)}
 *     .face{width:var(--fs,34px);height:var(--fs,34px);border-radius:calc(var(--fs,34px)/3);
 *       …color:var(--c);font-family:var(--mono);font-weight:700;…}
 *     <div class="who"><span class="face a-scout" style="--fs:22px">Sc</span>Scout…</div>
 *
 * Czyli: TWARZ o boku 22 px, z inicjałami w środku i w barwie tożsamości tego agenta, a obok
 * niej nazwa napisana krojem do CZYTANIA. Karta rysowała zamiast tego kwadracik 11 px, PUSTY
 * w środku, a nazwę pisała krojem maszynowym w stopniu etykiety — czyli tak, jak ta aplikacja
 * pisze WARTOŚCI WYLICZONE (`.value`, `--text-label`), a nie imiona.
 *
 * DLACZEGO TO NIE JEST GUST. Kwadracik bez ani jednego znaku w środku niesie tożsamość
 * WYŁĄCZNIE barwą, a pięć tokenów `--color-id-*` to pięć przygaszonych odcieni różniących się
 * o kilkanaście stopni. Osoba, która ich nie rozdziela, dostaje z tego wiersza dokładnie nic —
 * i jest to ten sam brak, na który ten katalog odpowiedział już raz, dostawiając SŁOWO do chipa
 * stanu (`./the-state-of-a-step-is-a-word.test.tsx`). Inicjały są ODCZYTEM nazwy, którą plan
 * już niesie, a nie nowym faktem (niezmiennik 17): kiedy nazwy nie ma, nie ma i inicjałów.
 *
 * PRZEZ `RunGraph`, NIE PRZEZ SAM KAFELEK — ten sam powód, co w pliku obok: kafelek
 * wyrenderowany wprost przechodzi także wtedy, gdy nic go nigdy nie montuje (niezmiennik 29).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { GraphStep, Plan } from './model';
import { RunGraph } from './graph';

/** Dwa kroki: wykonawca o nazwie z jednego słowa i wykonawca o nazwie z dwóch. */
const PLAN: Plan = {
  steps: [
    {
      id: 's1',
      name: 'Tests pass',
      status: 'working',
      who: { name: 'Forge', square: '--color-id-3' },
      doing: 'Running the checks the project already has.',
    },
    {
      id: 's2',
      name: 'Second opinion',
      status: 'waiting',
      who: { name: 'Second reader', square: '--color-id-4' },
    } satisfies GraphStep,
    /* KSZTAŁT PRAWDZIWEGO BIEGU: plan zeruje nazwę wykonawcy, kiedy agent nazywa się tak jak
       krok — a nazywa się tak zawsze (`../index.tsx`, `planFor`). Zmierzone w chromium. */
    {
      id: 's3',
      name: 'Tests pass',
      status: 'working',
      who: { name: '', square: '--color-id-5' },
      doing: '214 checks. 212 passed, 2 failed.',
    } satisfies GraphStep,
  ],
  links: [],
};

const MARKUP = renderToStaticMarkup(<RunGraph plan={PLAN} />);

/**
 * Znacznik tożsamości o tej barwie: jego znaczniki otwierające i to, CO MA W ŚRODKU.
 *
 * Wycinek liczony od tokenu barwy do najbliższego `<` po zamknięciu znacznika otwierającego —
 * czyli dokładnie ten fragment, w którym pusty element widać jako pusty napis. Kwadracik
 * `<i style="background:var(--color-id-3)"></i>` daje `''` i to jest cała treść tego pliku.
 */
function faceOf(token: string): { readonly opening: string; readonly inside: string } {
  const at = MARKUP.indexOf(`var(${token})`);
  if (at === -1) return { opening: '', inside: '' };
  const shuts = MARKUP.indexOf('>', at);
  const next = MARKUP.indexOf('<', shuts);
  return {
    opening: MARKUP.slice(Math.max(0, at - 300), shuts),
    inside: MARKUP.slice(shuts + 1, next === -1 ? shuts + 1 : next),
  };
}

/** Znacznik otwierający linii, która stoi ZA twarzą — czyli nazwy tego, kto robi ten krok. */
function nameBeside(token: string): string {
  const at = MARKUP.indexOf(`var(${token})`);
  const line = at === -1 ? -1 : MARKUP.indexOf('data-card-line', at);
  return line === -1 ? '' : MARKUP.slice(line, MARKUP.indexOf('>', line));
}

describe('kto robi ten krok, napisane nie samą barwą', () => {
  it('carries the identity colour this worker already has beside the step', () => {
    expect(
      MARKUP,
      'without the identity colour there is nothing here to read at all, and every point below ' +
        'would be looking at an empty string',
    ).toContain('var(--color-id-3)');
  });

  it('leaves nothing inside the identity mark for a one-word name — it should read Fo', () => {
    expect(
      faceOf('--color-id-3').inside,
      'the identity mark on the step card is an empty coloured box. The five identity colours ' +
        'are five dimmed hues a dozen degrees apart, so a person who does not separate them ' +
        'reads nothing at all from this row — the same loss this folder already answered once by ' +
        'putting a word beside the state. The mockup writes the initials inside the mark ' +
        '(`.face` carries "Sc" for Scout), and the name is already in the plan, so this is a ' +
        'reading of data we have and not a new fact',
    ).toBe('Fo');
  });

  it('leaves nothing inside the identity mark for a two-word name — it should read Sr', () => {
    expect(
      faceOf('--color-id-4').inside,
      'a worker called Second reader has to read as Sr, one letter per word — the policy the ' +
        'stream already owns for the very same mark (`../feed/who.ts`). Taking the first two ' +
        'letters of the whole string would print Se for both Scout and Second reader, and the ' +
        'two marks would stop telling two workers apart, which is the one thing they exist for',
    ).toBe('Sr');
  });

  it('leaves the mark blank when the plan blanks the name — it should sign with the step', () => {
    expect(
      faceOf('--color-id-5').inside,
      'the plan empties the name of a worker that is called the same thing as its step, and in ' +
        'a real run it is called that every time — measured in chromium on 2026-08-31, both ' +
        'cards the stream had spoken about came through with an empty name and a blank mark. ' +
        'An empty name here means "this worker is named after this step", never "we do not ' +
        'know who this is", so the mark signs with the step and never with a question mark. ' +
        'The colour of this very mark is hashed from that same string, so the letters and the ' +
        'colour are then talking about one name and not two',
    ).toBe('Tp');
  });

  it('gives the identity mark the size the mockup gives it, not the size of a dot', () => {
    expect(
      faceOf('--color-id-3').opening,
      'the mockup draws this mark at 22 px (`.face` at `--fs:22px`) with a corner radius of a ' +
        'third of that, which is what makes it a face and not a bullet. The card drew an 11 px ' +
        'square with square corners, which reads as a list marker and has no room for a letter',
    ).toContain('size-[22px]');
  });

  it("writes the worker's name in the reading font, not the one kept for measured values", () => {
    expect(
      nameBeside('--color-id-3'),
      'the name of the person doing this step was drawn in the monospace face at label size — ' +
        'which is exactly how this application draws COMPUTED VALUES (`.value`, the last line ' +
        'of this very card). A name is not a measurement. The mockup writes `.who` in the ' +
        'reading font at 12 px in body colour and keeps the monospace face for the model name ' +
        'beside it (`.mo`)',
    ).not.toContain('font-mono');
  });
});
