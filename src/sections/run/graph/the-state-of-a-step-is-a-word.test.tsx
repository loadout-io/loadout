/* Stan kroku jest SŁOWEM, nie tylko kształtem — i to jest kryterium dostępności, nie estetyki.
 *
 * ZMIERZONA STRATA. Do 2026-08-31 stan agenta stał SŁOWEM w kolumnie agentów, w kolorze
 * nasyconym. Kolumna zeszła z ekranu, a kafelek planu, który ją zastąpił, mówił stan WYŁĄCZNIE
 * formą: sześć rozłącznych kompletów klas plus glif z `aria-hidden`. Osoba, która nie odróżnia
 * przygaszonych barw, traciła tę informację w całości — a jest to jedyna rzecz na tym ekranie,
 * po którą się na niego patrzy.
 *
 * DLACZEGO TEKST, A NIE `aria-label`. Podpis dla czytnika ekranu odpowiada na ślepotę, nie na
 * daltonizm: osoba, która widzi kafelek i nie odróżnia dwóch przygaszonych odcieni, nie ma
 * włączonego czytnika i nigdy tego zdania nie usłyszy. Nośnikiem musi być więc widoczny napis.
 *
 * DLACZEGO PRZEZ `RunGraph`, A NIE PRZEZ SAM KAFELEK. Kafelek wyrenderowany wprost przechodzi
 * także wtedy, gdy nic go nigdy nie montuje (niezmiennik 29). Plan bez pozycji i bez strzałek
 * rysuje LISTĘ kroków tym samym kafelkiem, więc lista jest drogą, po której człowiek ten napis
 * naprawdę widzi.
 *
 * ZNACZNIKI SĄ ZDEJMOWANE PRZED PORÓWNANIEM, i to jest cała treść tego pliku: stan zapisany
 * w nazwie klasy albo w atrybucie przechodzi kryterium czytające surowy markup, a człowiek go
 * nie widzi. Zostaje wyłącznie to, co da się przeczytać.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { AgentStatus } from '../rail/card';
import type { GraphStep, Plan } from './model';
import { RunGraph } from './graph';

const SIX: readonly AgentStatus[] = [
  'waiting',
  'working',
  'needs you',
  'done',
  'failed',
  'stopped',
];

/* Ten sam krok sześć razy, różny WYŁĄCZNIE stanem: nazwa, wykonawca i zdanie są równe
 * z konstrukcji, więc wszystko, co odróżnia te sześć kafelków, mówi o stanie i o niczym
 * innym. */
const PLAN: Plan = {
  steps: SIX.map((status, at): GraphStep => ({
    id: `s${String(at)}`,
    name: 'Build the parser',
    status,
    who: { name: 'Forge', square: '--color-id-3' },
    doing: 'Rewriting the quote handling as a small machine with three positions.',
  })),
  links: [],
};

const MARKUP = renderToStaticMarkup(<RunGraph plan={PLAN} />);

/** Markup każdego kafelka z osobna, w kolejności planu, bez jego klucza. */
function cards(markup: string): readonly string[] {
  return markup
    .split('data-step="')
    .slice(1)
    .map((chunk) => chunk.slice(chunk.indexOf('"') + 1));
}

/**
 * To, co człowiek na tym kafelku PRZECZYTA — bez znaczników i bez atrybutów.
 *
 * Najpierw odcinamy resztę znacznika otwierającego (kawałek zaczyna się w środku niego, zaraz
 * za `data-step`), potem znikają wszystkie znaczniki razem z klasami. Bez tego pierwszego cięcia
 * komplet klas pierwszego kafelka zostałby w tekście i punkt niżej sądziłby nazwę klasy.
 */
function textOf(piece: string): string {
  return (
    piece
      .slice(piece.indexOf('>') + 1)
      .replace(/<[^>]*>/g, ' ')
      /* Ostatni kawałek urywa się w środku znacznika NASTĘPNEGO kafelka, bo cięcie idzie po
       * `data-step`. Niedomknięty ogon nie jest tekstem i nie ma prawa wejść do porównania. */
      .replace(/<[^>]*$/, ' ')
      .replace(/\s+/g, ' ')
      .trim()
  );
}

const DRAWN = cards(MARKUP);
const wordsOn = (status: AgentStatus): string => textOf(DRAWN[SIX.indexOf(status)] ?? '');

describe('stan kroku da się przeczytać', () => {
  it('draws one card per step, so everything below has something to read', () => {
    expect(
      DRAWN.length,
      'the plan carries six steps and the markup has ' +
        String(DRAWN.length) +
        ' cards, so every point below would be reading an empty string and passing on nothing',
    ).toBe(6);
  });

  it('leaves real readable text on each card once the markup is stripped away', () => {
    for (const status of SIX) {
      expect(
        wordsOn(status),
        'stripping the markup off the card for a step that is ' +
          status +
          ' left nothing to read, so the points below would be looking at an empty string',
      ).toContain('Build the parser');
    }
  });

  it('says in words which of the six states each step is in', () => {
    for (const status of SIX) {
      expect(
        wordsOn(status),
        'the card for a step that is ' +
          status +
          ' never says so in text. It reads: ' +
          JSON.stringify(wordsOn(status)) +
          '. Six disjoint sets of classes and a glyph nobody announces are shape only, and a ' +
          'person who cannot tell two dimmed hues apart loses the one fact this screen exists ' +
          'to carry. Colour may repeat it; colour may not be the only place it lives.',
      ).toContain(status);
    }
  });

  it('never puts the word of one state on the card of another', () => {
    for (const status of SIX) {
      const said = wordsOn(status);
      const wrong = SIX.filter((other) => other !== status && said.includes(other));
      expect(
        wrong,
        'the card for a step that is ' +
          status +
          ' also reads the words ' +
          JSON.stringify(wrong) +
          '. A card that names every state names none of them, and printing all six is the ' +
          'cheapest way to pass the point above while saying nothing',
      ).toEqual([]);
    }
  });

  it('keeps every card at four lines of text, never five', () => {
    for (const status of SIX) {
      const piece = DRAWN[SIX.indexOf(status)] ?? '';
      const lines = [...piece.matchAll(/\bdata-card-line\b/g)].length;
      expect(
        lines,
        'the card for a step that is ' +
          status +
          ' carries ' +
          String(lines) +
          ' lines of text. Four is the ceiling [ARCHITECTURE §7], and the word about the state ' +
          'has to find room inside those four rather than become a fifth',
      ).toBe(4);
    }
  });
});
