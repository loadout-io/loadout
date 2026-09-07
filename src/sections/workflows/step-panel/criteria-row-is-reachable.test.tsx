/* Człowiek może zatwierdzić listę wymagań — bez edytowania JSON-a.
 *
 * # Z czego to wyszło
 *
 * Prawdziwy bieg z 2026-09-06: krok QA opisał w prozie brak obowiązkowych zachowań i w tej samej
 * odpowiedzi napisał, że praca przeszła. Nie złamał instrukcji — pytano go, czy praca jest „good
 * enough to build on". Rust liczy dziś wynik z KOMPLETNOŚCI zatwierdzonej listy, a nie z tego
 * jednego słowa; ta lista musi mieć skąd się wziąć.
 *
 * # Dlaczego to jest kryterium, a nie szczegół
 *
 * Niezmiennik 16 czytany w drugą stronę: pole bez kontrolki nie jest funkcją produktu. Pole
 * `criteria` stoi w modelu Rusta i TypeScriptu, jedzie do promptu weryfikatora i rozstrzyga
 * werdykt pętli — a bez tego wiersza jedyną drogą do jego ustawienia byłoby dopisanie ręką
 * do pliku workflow.
 *
 * # Czego pilnuje najmocniej
 *
 * Metody. Bez niej weryfikator wolno obniża pomiar: „nagranie startuje po Later" potwierdzone
 * ekranem z podstawionym backendem odpowiada na inne pytanie niż to samo potwierdzone
 * uruchomioną aplikacją. Rust odmawia zaliczenia słabszego potwierdzenia; ten wiersz jest
 * jedynym miejscem, w którym człowiek mówi, czego chce.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Criterion } from '../../../state/workflows';
import { CriteriaRow } from './criteria-row';

function markup(value: readonly Criterion[] | undefined): string {
  return renderToStaticMarkup(
    <CriteriaRow
      value={value}
      onEditStep={() => {
        /* To kryterium pyta o markup, nie o skutek zmiany. */
      }}
    />,
  );
}

const ONE: readonly Criterion[] = [
  {
    id: 'c2',
    behaviour: 'The next recording starts after Later',
    required: true,
    method: 'full-runtime',
  },
];

describe('approving what a step must confirm', () => {
  it('is on the panel at all, or the list lives only in the file', () => {
    expect(
      markup(undefined).includes('data-row="criteria"'),
      'without a control the only way to approve a requirement is editing the workflow JSON by ' +
        'hand — and a list nobody can reach is a list the product does not have',
    ).toBe(true);
  });

  it('says what an empty list means, so leaving it empty is a choice', () => {
    expect(
      markup(undefined),
      'a person who leaves this empty gets the old behaviour — one word at the end decides — ' +
        'and has to be told that before reading the empty row as a requirement',
    ).toContain('answers in one word at the end and that word decides');
  });

  it('shows every approved requirement with its wording and the way it must be confirmed', () => {
    const shown = markup(ONE);
    expect(shown).toContain('value="c2"');
    expect(shown).toContain('value="The next recording starts after Later"');
    expect(
      shown,
      'the way a requirement must be confirmed is missing, so a verifier can answer it with ' +
        'stand-in data and nobody sees the difference',
    ).toContain('The running application with its real backend');
  });

  it('offers every way of confirming, or the choice cannot be made', () => {
    const shown = markup(ONE);
    for (const method of ['automated-test', 'mocked-ui', 'full-runtime', 'human-confirmed']) {
      expect(shown, `the way "${method}" is not among the ones offered`).toContain(
        `value="${method}"`,
      );
    }
  });

  it('hands the edited list up, and an emptied list back as no list at all', () => {
    const edits: { criteria: Criterion[] | undefined }[] = [];
    const row = CriteriaRow({
      value: ONE,
      onEditStep: (fields) => edits.push(fields),
    });
    const found = handlers(row);

    expect(
      [found.behaviour, found.method, found.remove, found.add].filter((one) => one === undefined)
        .length,
      'a control in the rendered row has no handler behind it. It looks on screen exactly like ' +
        'one that works, and nothing the person types reaches the file (invariant 16).',
    ).toBe(0);

    found.behaviour?.({ target: { value: 'Audio is kept' } });
    found.method?.({ target: { value: 'mocked-ui' } });
    found.add?.();
    found.remove?.();

    expect(edits.at(0)?.criteria?.at(0)?.behaviour).toBe('Audio is kept');
    expect(edits.at(1)?.criteria?.at(0)?.method).toBe('mocked-ui');
    expect(edits.at(2)?.criteria).toHaveLength(2);
    expect(
      edits.at(3)?.criteria,
      'an emptied list has to leave the file without the key, so the step goes back to exactly ' +
        'the contract it had before anyone approved anything',
    ).toBeUndefined();
  });
});

/** Uchwyty kontrolek wyjęte z drzewa — `renderToStaticMarkup` oddaje napis, a napis ich nie ma. */
function handlers(tree: unknown): {
  behaviour?: (event: { target: { value: string } }) => void;
  method?: (event: { target: { value: string } }) => void;
  remove?: () => void;
  add?: () => void;
} {
  const found: ReturnType<typeof handlers> = {};
  const walk = (node: unknown): void => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) {
      node.forEach(walk);
      return;
    }
    const element = node as { props?: Record<string, unknown> };
    const props = element.props;
    if (props) {
      const field = props['data-field'];
      if (field === 'criterion-behaviour') found.behaviour = props['onChange'] as never;
      if (field === 'criterion-method') found.method = props['onChange'] as never;
      if (typeof props['children'] === 'string' && props['onClick']) {
        if (props['children'] === 'Remove') found.remove = props['onClick'] as never;
        if (props['children'] === '+ Ask it to confirm one more')
          found.add = props['onClick'] as never;
      }
      walk(props['children']);
    }
    return;
  };
  walk(tree);
  return found;
}
