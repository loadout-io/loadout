/* Kryterium 5 dla T-25: zepsuty ekran kosztuje jedną sekcję, nie całe okno.
 *
 * `42 as never` w miejscu komponentu to nie jest wymyślona sytuacja: dokładnie tak wygląda plik
 * sekcji bez domyślnego eksportu, moduł w połowie napisany i literówka w nazwie eksportu.
 * `as never` jest tu jedynym sposobem, żeby TypeScript przepuścił to, co na dysku zdarza się
 * samo — kompilator pilnuje naszych plików, nie cudzych.
 *
 * `expect(() => render()).not.toThrow()` samo w sobie przechodzi na implementacji, która łyka
 * wyjątek i renderuje pustego `<main>` bez zdania. Użytkownik widzi wtedy biały prostokąt i nie
 * wie, czy aplikacja jest zepsuta, czy pusta — a to jest gorsze niż awaria, bo nie da się tego
 * zgłosić. Odróżnia je obecność ZDANIA Z REJESTRU w tym samym dokumencie: pusty ekran jest
 * prawdziwą odpowiedzią, biały prostokąt nie jest.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import type { ScreenMap } from '../screens';
import { sectionEntry } from '../sections';

/** Wpis, którego nie da się wyrenderować: liczba tam, gdzie powłoka spodziewa się komponentu. */
const BROKEN: ScreenMap = { knowledge: 42 as never };

/** Ta sama zepsuta sekcja obok zdrowej — jedna nie ma prawa zabrać drugiej. */
const BROKEN_AND_GOOD: ScreenMap = {
  knowledge: 42 as never,
  run: () => <p data-screen="run" />,
};

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Treść jedynego elementu z `data-empty`, bez znaczników i bez nadmiarowych odstępów. */
function emptyStateText(markup: string): string {
  const hit = /<([a-z]+)[^>]*\bdata-empty\b[^>]*>([\s\S]*?)<\/\1>/i.exec(markup);
  return (hit?.[2] ?? '')
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('a screen that cannot be rendered costs one section, not the window', () => {
  it('renders the knowledge section without falling over', () => {
    expect(
      () => renderToStaticMarkup(<App section="knowledge" screens={BROKEN} />),
      'a value that is not a component has to be treated as no screen at all. Throwing here ' +
        "takes down every section in the window over one section's file",
    ).not.toThrow();
  });

  it('says what an empty knowledge section says, instead of showing nothing', () => {
    const markup = renderToStaticMarkup(<App section="knowledge" screens={BROKEN} />);
    expect(
      occurrences(markup, 'data-empty'),
      'knowledge has nothing renderable, so it falls back to its empty screen — exactly one of them',
    ).toBe(1);
    expect(
      emptyStateText(markup),
      'the fallback is the sentence from the entry, read here from the registry. Swallowing ' +
        'the bad value and rendering nothing at all passes a not-toThrow check and leaves a ' +
        'white rectangle nobody can report',
    ).toBe(sectionEntry('knowledge').empty);
  });

  it('leaves a healthy section next to it untouched', () => {
    const markup = renderToStaticMarkup(<App section="run" screens={BROKEN_AND_GOOD} />);
    expect(
      occurrences(markup, 'data-screen="run"'),
      'the run screen is fine and has to render exactly once, whatever the knowledge entry holds',
    ).toBe(1);
    expect(
      occurrences(markup, 'data-empty'),
      'run has a screen, so there is no empty screen to show for it',
    ).toBe(0);
  });

  it('leaves a section with no screen at all untouched', () => {
    const markup = renderToStaticMarkup(<App section="triggers" screens={BROKEN_AND_GOOD} />);
    expect(
      emptyStateText(markup),
      'triggers has no screen here in this map, so it says what its entry says — a bad ' +
        'value under another key has to change nothing about it',
    ).toBe(sectionEntry('triggers').empty);
  });
});
