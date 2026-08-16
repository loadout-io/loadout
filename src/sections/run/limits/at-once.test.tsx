/* Kryterium 7 dla T-21: kontrolka „ile naraz" jest ograniczona, a ostrzeżenie o pamięci
 * jest WYLICZANE.
 *
 * Słaba wersja tego kryterium to sprawdzenie, że w markupie występuje słowo `GB`. Przechodzi
 * ją zdanie wpisane na sztywno, czyli dokładnie ta wersja kontrolki, która przy ośmiu agentach
 * dalej mówi „about 0.6 GB" i przez to nie ostrzega przed niczym.
 *
 * Rozstrzygają dwie rzeczy naraz: dwie różne wartości muszą dać dwa RÓŻNE zdania ostrzeżenia,
 * a wartość poniżej podpowiedzi musi dać ZERO elementów ostrzeżenia. Implementacja ze stałym
 * zdaniem pada na obu.
 *
 * Bez jsdom: `renderToStaticMarkup` z `react-dom/server`. Dopisanie `@testing-library/react`
 * to zmiana `package.json`, czyli moment na zatrzymanie się i zapytanie człowieka
 * (AGENTS.md §7).
 *
 * Czego ten plik NIE sprawdza: że handler naprawdę zmienia limit w biegu. Wymagany prop
 * `onChange` jest egzekwowany przez bramkę typów, a sklejenie kontrolki z biegiem należy
 * do T-07 i T-15.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { AtOnce } from './at-once';

/** Podpowiedź maszyny 16 GB: cztery agenty. Powyżej niej kontrolka ma ostrzegać. */
const SUGGESTED = 4;

function noop(): void {
  // Handler jest wymagany, ale to kryterium nie pyta, co robi.
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

function markupFor(value: number | undefined, suggested: number): string {
  return value === undefined
    ? renderToStaticMarkup(<AtOnce suggested={suggested} onChange={noop} />)
    : renderToStaticMarkup(<AtOnce value={value} suggested={suggested} onChange={noop} />);
}

/** Treść jedynego elementu z `data-at-once-warning`, bez znaczników i bez nadmiarowych odstępów. */
function warningText(markup: string): string {
  const hit = /<([a-z]+)[^>]*\bdata-at-once-warning\b[^>]*>([\s\S]*?)<\/\1>/i.exec(markup);
  return (hit?.[2] ?? '')
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('the control that says how many agents run at once', () => {
  it('opens on three when nothing was saved', () => {
    expect(
      markupFor(undefined, SUGGESTED),
      'with no saved choice the control has to stand at three — the number a 16 GB machine ' +
        'holds at 583 MB per agent',
    ).toContain('value="3"');
  });

  it('is bounded at both ends', () => {
    const markup = markupFor(undefined, SUGGESTED);
    expect(markup, 'one agent is the floor: zero is a run that never starts').toContain('min="1"');
    expect(
      markup,
      'eight is the ceiling, and it has to be part of the control rather than advice next to ' +
        'it — a plain number field with no top lets somebody type ten and freeze the machine',
    ).toContain('max="8"');
  });

  it('stays quiet at or below the suggestion', () => {
    for (const value of [1, 2, 3, SUGGESTED]) {
      expect(
        occurrences(markupFor(value, SUGGESTED), 'data-at-once-warning'),
        'at ' + String(value) + ' the machine holds this comfortably, so there is nothing to say',
      ).toBe(0);
    }
  });

  it('warns exactly once above the suggestion', () => {
    for (const value of [5, 6, 7, 8]) {
      expect(
        occurrences(markupFor(value, SUGGESTED), 'data-at-once-warning'),
        'at ' +
          String(value) +
          ' there has to be one warning and only one: one fact, one place on the screen',
      ).toBe(1);
    }
  });

  it('counts the memory rather than naming it', () => {
    const five = warningText(markupFor(5, SUGGESTED));
    const eight = warningText(markupFor(8, SUGGESTED));

    expect(
      five,
      'the element carrying data-at-once-warning has to hold the sentence itself, otherwise ' +
        'there is no way to tell a computed warning from a decorative one',
    ).not.toBe('');
    expect(five, 'five agents at 583 MB each').toContain('about 2.9 GB');
    expect(eight, 'eight agents at 583 MB each').toContain('about 4.7 GB');
    expect(
      five,
      'and the two have to differ. A sentence typed in by hand reads the same at five agents ' +
        'and at eight, which is the version that keeps saying about 0.6 GB while the machine ' +
        'runs out of memory',
    ).not.toBe(eight);
  });

  it('asks its question in plain words', () => {
    const markup = markupFor(undefined, SUGGESTED);
    expect(markup, 'the label is the question a person would ask').toContain(
      'How many agents at once?',
    );
    expect(markup, 'and the helper line says what the choice costs').toContain(
      'More agents finish sooner but use more memory.',
    );
  });
});
