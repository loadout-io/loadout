/* Człowiek może powiedzieć, czego następny krok potrzebuje — bez edytowania JSON-a.
 *
 * # Pytanie, z którego to wyszło
 *
 * Właściciel, 2026-08-30: „czy agenci są świadomi co po nich następuje… np że architekt do
 * dewelopera?". Z kodu: wstecz wiedzą wszystko — indeks przekazań podpisany NAZWĄ kafelka —
 * a w przód nie wiedzą nic poza tym, że ktoś jest.
 *
 * # Dlaczego kontrolka, a nie nazwy następników w promcie
 *
 * Bo sama nazwa roli jest cienka: „następny jest Tester" każe agentowi zgadywać, co z tego
 * wynika, a akapit doklejony do promptu rośnie u WSZYSTKICH i na zawsze (niezmiennik 28).
 * Rzecz, która naprawdę zmienia wynik, była w produkcie od dawna: formularz przekazania — pole
 * z opisem CZŁOWIEKA, o które agent jest wprost proszony.
 *
 * I nie miała ani jednej kontrolki. `handover` stało w modelu Rusta i TypeScriptu, jechało do
 * promptu, miało odmowę za brak pola wymaganego — a jedyną drogą do ustawienia go było ręczne
 * dopisanie do pliku workflow. To jest niezmiennik 16 czytany w drugą stronę: pole bez kontrolki
 * nie jest funkcją produktu.
 *
 * # Czego to kryterium pilnuje najmocniej
 *
 * Ostatniego przypadku: napis musi mówić o SKUTKU. „Needed" jako samo słowo jest etykietą; zdanie
 * „a needed one it leaves out stops the step" jest tym, czego człowiek potrzebuje, żeby wybrać
 * świadomie — bo brak takiego pola jest po stronie Rusta ODMOWĄ kroku, nie brakiem w odpowiedzi.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Handover } from '../../../state/workflows';
import { HandoverRow } from './handover-row';

function markup(value: Handover): string {
  return renderToStaticMarkup(
    <HandoverRow
      value={value}
      onEditStep={() => {
        /* To kryterium pyta o markup, nie o skutek zmiany. */
      }}
    />,
  );
}

describe('saying what the next step needs', () => {
  it('is on the panel at all, or the field lives only in the file', () => {
    expect(
      markup('notes').includes('data-field="handover"'),
      'without a control the only way to ask for a field is editing the workflow JSON by hand — ' +
        'and a field nobody can reach is a field the product does not have',
    ).toBe(true);
  });

  it('offers both shapes and marks the one in force', () => {
    const plain = markup('notes');
    expect(
      /name="handover-shape"[^>]*checked/.test(plain) || plain.includes('checked'),
      'prose',
    ).toBe(true);

    const named = markup({ fields: [{ name: 'plan', describe: 'the order of changes' }] });
    expect(named.includes('value="plan"')).toBe(true);
    expect(named.includes('value="the order of changes"')).toBe(true);
  });

  it('says the fields are EXTRA, not instead of the answer', () => {
    expect(
      markup({ fields: [{ name: 'plan', describe: 'x' }] }).includes(
        'It still answers under the three headings',
      ),
      'Rust asks for these with "This step ALSO has to hand these back". A control that reads ' +
        'like a replacement promises a shape the agent will not produce',
    ).toBe(true);
  });

  it('says what happens when a needed one is missing', () => {
    expect(
      markup({ fields: [{ name: 'plan', describe: 'x', required: true }] }).includes(
        'stops the step',
      ),
      '"Needed" on its own is a label. A field marked needed and left out REFUSES the step in ' +
        'Rust, and the person picking the checkbox has to know that before they pick it',
    ).toBe(true);
  });

  it('never renders a field editor while the step hands over prose', () => {
    expect(
      markup('notes').includes('data-field="handover-name"'),
      'an empty field editor under an unselected mode is a control that looks like it applies ' +
        'and does not',
    ).toBe(false);
  });
});
