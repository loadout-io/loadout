/* Okno palety w markupie: to, co czytnik ekranu ma usłyszeć, i to, co człowiek ma zobaczyć.
 *
 * DLACZEGO MARKUP, A NIE WARTOŚĆ ZWRÓCONA PRZEZ FUNKCJĘ (niezmiennik 29). `role="dialog"`
 * i `aria-modal` są obietnicą wobec czytnika ekranu i wobec przeglądarki; obietnica złożona
 * w obiekcie, którego nikt nie renderuje, nie dociera do nikogo. Ten plik czyta więc dokument,
 * który powstaje naprawdę.
 *
 * CZEGO TEN PLIK NIE DOWODZI I GDZIE TO STOI. `renderToStaticMarkup` nigdy nie odpala
 * `onKeyDown` ani `onMouseDown`, więc pułapka ogniska, powrót ogniska i zamknięcie klikiem
 * w tło są tu NIEWIDZIALNE. Dowodzi ich `e2e/tests/the-keyboard-reaches-every-section.spec.ts`
 * — prawdziwa przeglądarka, prawdziwe naciśnięcie. Podział jest ten sam, co w całym repo:
 * markup mówi, że zdanie JEST, przeglądarka mówi, że da się do niego dojść.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { paletteItems } from './items';
import { Palette } from './palette';
import { shortcuts } from './shortcuts';

const NOTHING = (): void => undefined;

const ITEMS = paletteItems(
  [{ id: 'ship-a-feature.json', label: 'Ship a feature' }],
  [{ id: 'agent-1', label: 'Reviewer' }],
);

function drawn(at = 0, unread = false): string {
  return renderToStaticMarkup(
    <Palette
      showing="items"
      typed=""
      items={ITEMS}
      rows={shortcuts()}
      at={at}
      unread={unread}
      onType={NOTHING}
      onStep={NOTHING}
      onChoose={NOTHING}
      onShow={NOTHING}
      onClose={NOTHING}
    />,
  );
}

/** Ile razy ten podciąg stoi w markupie. */
function times(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

describe('the palette is a window a person can get out of', () => {
  it('announces itself as a window that holds everything while it is up', () => {
    const markup = drawn();
    expect(markup).toContain('role="dialog"');
    expect(markup).toContain('aria-modal="true"');
    /* Nazwa okna, nie tylko jego rola: dialog bez nazwy czyta się jako „okno dialogowe"
       i tyle, więc człowiek, który go nie widzi, nie wie, co przed nim stoi. */
    expect(markup).toMatch(/aria-label="[^"]+"/);
  });

  it('has one place to type into, drawn as the field this house already owns', () => {
    const markup = drawn();
    expect(times(markup, '<input')).toBe(1);
    expect(markup).toContain('class="field"');
    expect(markup).toMatch(/placeholder="[^"]+"/);
  });

  it('draws every position as a row of the list, with exactly one of them highlighted', () => {
    const markup = drawn(2);
    expect(times(markup, 'role="option"')).toBe(ITEMS.length);
    expect(times(markup, 'aria-selected="true"')).toBe(1);
    expect(times(markup, 'class="row"')).toBe(ITEMS.length);
    /* Podświetlenie ma być TĄ pozycją, którą wskazano, a nie pierwszą z brzegu. */
    const highlighted = /<li[^>]*aria-selected="true"[^>]*>([\s\S]*?)<\/li>/.exec(markup);
    expect((highlighted?.[1] ?? '').replace(/<[^>]*>/g, ' ')).toContain(ITEMS[2]?.label ?? '');
  });

  it('names the saved things it found next to the sections it always knows', () => {
    const markup = drawn();
    expect(markup).toContain('Ship a feature');
    expect(markup).toContain('Reviewer');
    expect(markup).toContain('data-palette-item="workflow"');
    expect(markup).toContain('data-palette-item="agent"');
  });

  it('has a backdrop to click and a control that leads to the shortcuts', () => {
    const markup = drawn();
    expect(markup).toContain('data-palette-backdrop');
    expect(markup).toContain('data-palette-show="shortcuts"');
  });

  it('says out loud when the saved things could not be read, instead of looking empty', () => {
    expect(drawn(0, false)).not.toContain('data-palette-unread');
    const refused = drawn(0, true);
    expect(refused).toContain('data-palette-unread');
    const sentence = /data-palette-unread[^>]*>([\s\S]*?)<\/p>/.exec(refused);
    expect((sentence?.[1] ?? '').trim().length).toBeGreaterThan(0);
  });

  it('says so when a typed word leaves nothing standing', () => {
    const empty = renderToStaticMarkup(
      <Palette
        showing="items"
        typed="nothing by that name"
        items={[]}
        rows={shortcuts()}
        at={0}
        unread={false}
        onType={NOTHING}
        onStep={NOTHING}
        onChoose={NOTHING}
        onShow={NOTHING}
        onClose={NOTHING}
      />,
    );
    expect(empty).toContain('data-palette-empty');
    expect(times(empty, 'role="option"')).toBe(0);
  });
});
