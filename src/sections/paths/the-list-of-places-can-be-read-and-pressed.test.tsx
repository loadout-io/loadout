/* Lista miejsc daje się przeczytać czytnikiem ekranu i nacisnąć.
 *
 * Kryterium pyta o to, CO SIĘ WYRENDEROWAŁO, a nie o kształt komponentu: pole wskazuje wybraną pozycję
 * przez `aria-activedescendant`, więc identyfikatory po obu stronach muszą się zgadzać co do znaku.
 * Rozjazd tam jest niewidzialny na ekranie i całkowicie głuchy dla czytnika.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { AtPicker, optionId } from './at-picker';

const PLACES = [
  { path: 'src/', folder: true },
  { path: 'README.md', folder: false },
] as const;

describe('the list of places can be read and pressed', () => {
  it('draws nothing at all when there is nothing to suggest', () => {
    expect(
      renderToStaticMarkup(<AtPicker items={[]} active={0} id="x" onChoose={() => undefined} />),
      'an empty list still put something on the page. An empty box under the field is chrome ' +
        'that answers no question, and it shifts the layout every time a person types a ' +
        'character that matches nothing.',
    ).toBe('');
  });

  it('says it is a list, and says which entry is chosen', () => {
    const drawn = renderToStaticMarkup(
      <AtPicker items={PLACES} active={1} id="places" onChoose={() => undefined} />,
    );

    expect(drawn, 'the list does not announce itself as one').toContain('role="listbox"');
    expect(
      drawn,
      'the chosen entry is not marked, so a screen reader reads the same sentence whichever ' +
        'entry the arrows are on.',
    ).toContain('aria-selected="true"');
    expect(
      drawn,
      'the chosen entry does not carry the id the field points at with aria-activedescendant, ' +
        'so the pairing is silently broken.',
    ).toContain(`id="${optionId('places', 1)}"`);
  });

  it('shows a folder as a place you can enter, not as a file', () => {
    const drawn = renderToStaticMarkup(
      <AtPicker items={PLACES} active={0} id="places" onChoose={() => undefined} />,
    );

    expect(drawn, 'the folder is not marked as one').toContain('data-folder="yes"');
    expect(drawn, 'the file was marked as a folder').toContain('data-folder="no"');
    expect(drawn, 'the folder lost the trailing slash that says it can be entered').toContain(
      'src/',
    );
  });
});
