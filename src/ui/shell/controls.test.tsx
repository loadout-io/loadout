/* Kryterium 6 dla T-01: nie ma martwych przycisków.
 *
 * „Każda sekcja renderuje przycisk" przechodzi na powłoce z pięcioma przyciskami `Create`, które
 * nic nie robią — to jest dosłownie stan poprzedniego prototypu (`Export receipt`, `Copy`, `Review decision →`
 * bez handlera) i na zrzucie ekranu wygląda lepiej niż wersja poprawna [raport 03 §7.3].
 *
 * Rozróżniają je dwie rzeczy: DOKŁADNA liczba pięciu przycisków w każdej sekcji — czyli sam
 * przełącznik, i ani jednego więcej — oraz pytanie do store'a, czy przełączenie naprawdę
 * zmieniło wartość.
 *
 * W T-01 nie ma jeszcze czego tworzyć, więc pusta sekcja NIE MA przycisku. Ma jedno zdanie:
 * pusty ekran jest zaproszeniem, nie akapitem polityki (DESIGN.md §6).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import { useSectionStore } from './section-store';

const EXPECTED = ['run', 'workflows', 'agents', 'skills', 'memory'] as const;

/** Pusty ekran to zaproszenie: jedno zdanie, do dwunastu słów, najwyżej jedna kropka. */
const MOST_WORDS = 12;
const MOST_STOPS = 1;

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

function markupFor(id: (typeof EXPECTED)[number]): string {
  return renderToStaticMarkup(<App section={id} />);
}

/** Treść jedynego elementu z `data-empty`, bez znaczników i bez nadmiarowych odstępów. */
function emptyStateText(markup: string): string {
  const hit = /<([a-z]+)[^>]*\bdata-empty\b[^>]*>([\s\S]*?)<\/\1>/i.exec(markup);
  return (hit?.[2] ?? '')
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('the only controls are the five section switches, and each one really switches', () => {
  for (const id of EXPECTED) {
    it('shows five controls and no sixth, with ' + id + ' open', () => {
      const markup = markupFor(id);
      expect(
        occurrences(markup, '<button'),
        'with ' +
          id +
          ' open the shell has to render exactly five controls — the switcher, nothing else. ' +
          'A sixth one here is a Create that has nothing to create yet, which is a control ' +
          'without a handler (invariant 16)',
      ).toBe(EXPECTED.length);
      for (const other of EXPECTED) {
        expect(
          occurrences(markup, 'data-section-switch="' + other + '"'),
          'each of the five controls has to carry data-section-switch with a different one of ' +
            'the five names; ' +
            other +
            ' is missing or doubled',
        ).toBe(1);
      }
    });

    it('greets an empty ' + id + ' with one short sentence', () => {
      const markup = markupFor(id);
      expect(
        occurrences(markup, 'data-empty'),
        id + ' has to render exactly one element carrying data-empty',
      ).toBe(1);
      const sentence = emptyStateText(markup);
      const words = sentence.split(/\s+/).filter((word) => word.length > 0);
      expect(
        words.length,
        'the invitation on an empty screen has to fit in ' +
          String(MOST_WORDS) +
          ' words; ' +
          id +
          ' says: ' +
          JSON.stringify(sentence),
      ).toBeLessThanOrEqual(MOST_WORDS);
      expect(
        occurrences(sentence, '.'),
        'one sentence, so at most ' +
          String(MOST_STOPS) +
          ' full stop. An empty screen is an invitation, not a paragraph of policy; ' +
          id +
          ' says: ' +
          JSON.stringify(sentence),
      ).toBeLessThanOrEqual(MOST_STOPS);
    });
  }

  it('starts on Run and really moves when a switch is used', () => {
    expect(
      useSectionStore.getState().section,
      'the shell opens on run — the place where work happens',
    ).toBe('run');
    useSectionStore.getState().go('agents');
    expect(
      useSectionStore.getState().section,
      'go has to change the value the shell reads. A handler that is wired up and does nothing ' +
        'looks identical in the markup and identical in a screenshot',
    ).toBe('agents');
  });
});
