/* Kryterium 5 dla T-11: formularz agenta to dziewięć wierszy, a `More settings` to dokładnie
 * trzy.
 *
 * Słaba wersja tego kryterium to dziewięć osobnych `expect(html).toContain('Thinking')`.
 * Dziesiąte pole przechodzi wtedy bez mrugnięcia — a dokładnie o to tu pytamy. Przechodzi też
 * samo słowo `Thinking` w zdaniu pomocniczym pod kontrolką, czyli tekst, który niczego nie
 * zapisuje. Dlatego niżej stoi równość CAŁEJ tablicy etykiet, z kolejnością i długością.
 *
 * To jest jedyna obrona przed awarią, która nie boli ani jednego dnia i boli po trzech
 * miesiącach: każde pole da się uzasadnić pojedynczo (`temperature`, `maxTurns`, `retries`,
 * `workingDir`), a suma to strona ustawień poprzedniego prototypu z 28 atrybutami, których nikt nie tyka.
 *
 * Druga asercja pyta, czy trzy schowane wiersze są POZA drzewem, a nie tylko poza wzrokiem.
 * Kontrolka schowana `hidden`-em albo `display:none` dalej jest w HTML, dalej ma etykietę
 * i dalej rośnie — a na zrzucie ekranu wygląda identycznie jak wersja poprawna.
 *
 * Kolejność i brzmienia są wiążące: `docs/mockup/index.html`, panel `Forge` w sekcji Agents
 * (wiersze 683-691 w tej wersji pliku) plus tabela „We say / We never say" z T4 §8.1.
 * Rozstrzygnięcie sprzeczności, której implementacja nie ma podejmować sama: makieta ma
 * `Colour` jako osobny wiersz i nie ma `Write results to`, T4 §8.1 odwrotnie. Wygrywa makieta
 * — jest zatwierdzona — a `writeResultsTo` zostaje w typie i jest ustawiane na kroku, bo
 * ścieżka wyniku należy do kroku, nie do roli.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Agent } from '../../state/agents';
import { AgentForm } from './agent-form';

const NINE = [
  'Name',
  'What it does',
  'Colour',
  'Instructions',
  'Runs with',
  'Model',
  'Thinking',
  'Can it change files',
  'Give up after',
];

const THREE = ['Tools', 'Skills', 'Connections'];

const FORGE: Agent = {
  schema: 1,
  id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f77',
  name: 'Forge',
  summary: 'Writes code',
  color: 'clay',
  instructions: 'Write the smallest change that makes the checks pass.',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'work-freely',
  giveUpAfterMinutes: 20,
  tools: 'everything',
  skills: [],
  connections: [],
  writeResultsTo: 'handoffs/build.md',
};

function noop(): void {
  /* sterowany formularz: w statycznym renderze nic tego nie woła */
}

function markupOf(value: Agent, expanded: boolean): string {
  return renderToStaticMarkup(
    <AgentForm
      value={value}
      expanded={expanded}
      onChange={noop}
      onToggleMore={noop}
      onSave={noop}
    />,
  );
}

/** Tekst bez znaczników i bez encji. React zapisuje apostrof jako `&#x27;`. */
function plain(fragment: string): string {
  return fragment
    .replace(/<[^>]*>/g, ' ')
    .replace(/&#x27;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Teksty wszystkich `<label>` w kolejności wystąpienia. */
function labelsOf(html: string): string[] {
  return [...html.matchAll(/<label\b[^>]*>([\s\S]*?)<\/label>/g)].map((hit) => plain(hit[1] ?? ''));
}

/** Atrybuty przycisku o tym napisie, albo `null`, kiedy takiego przycisku nie ma. */
function buttonAttributes(html: string, label: string): string | null {
  for (const hit of html.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)) {
    if (plain(hit[2] ?? '') === label) return hit[1] ?? '';
  }
  return null;
}

describe('the agent form is nine rows, and More settings is exactly three', () => {
  it('reads out the nine labels of the mockup, in order, and no tenth one', () => {
    expect(
      labelsOf(markupOf(FORGE, false)),
      'these nine, in this order, and nothing else. A tenth row here is the first step towards ' +
        'the settings page nobody fills in, and it is always defensible on its own',
    ).toEqual(NINE);
  });

  it('adds exactly the three collapsed rows when More settings is open', () => {
    expect(
      labelsOf(markupOf(FORGE, true)),
      'open, the form is the same nine rows plus exactly Tools, Skills and Connections, in ' +
        'that order. Thirteen labels, not twelve and not fourteen',
    ).toEqual([...NINE, ...THREE]);
  });

  it('leaves the three collapsed rows out of the tree, not merely out of sight', () => {
    const html = markupOf(FORGE, false);

    expect(
      labelsOf(html).length,
      'the closed form is nine rows. Without this line the three checks below would also pass ' +
        'for a form that renders nothing at all',
    ).toBe(NINE.length);

    for (const field of ['tools', 'skills', 'connections']) {
      expect(
        html,
        'with More settings closed there is no control for ' +
          field +
          ' at all. A control that is in the page but hidden still counts as a row of this ' +
          'form, and it is how nine quietly becomes twelve',
      ).not.toContain('data-field="' + field + '"');
    }

    expect(
      / hidden(?:=""|>|\s)/.test(html),
      'nothing in this form may carry the hidden attribute: hiding is how a control stays in ' +
        'the page while the count above still passes',
    ).toBe(false);
    expect(
      /display\s*:\s*none/i.test(html),
      'and nothing may set display:none, for the same reason. What is open is decided in ' +
        'TypeScript, not in a style sheet',
    ).toBe(false);
  });

  it('will not let you save an agent with no name', () => {
    const attributes = buttonAttributes(markupOf({ ...FORGE, name: '' }, false), 'Save');

    expect(attributes, 'the form has to render a Save button; there is none').not.toBeNull();
    expect(
      /\bdisabled\b/.test(attributes ?? ''),
      'an agent with no name is not saveable: the name is how every other screen refers to it',
    ).toBe(true);
  });

  it('will not let you save an agent with no instructions', () => {
    const attributes = buttonAttributes(markupOf({ ...FORGE, instructions: '' }, false), 'Save');

    expect(attributes, 'the form has to render a Save button; there is none').not.toBeNull();
    expect(
      /\bdisabled\b/.test(attributes ?? ''),
      'instructions are 80% of what makes an agent an agent; an agent without them is a name',
    ).toBe(true);
  });

  it('lets you save as soon as the name and the instructions are filled in', () => {
    const attributes = buttonAttributes(markupOf(FORGE, false), 'Save');

    expect(attributes, 'the form has to render a Save button; there is none').not.toBeNull();
    expect(
      /\bdisabled\b/.test(attributes ?? ''),
      'everything except the name and the instructions has a default, so Save has to come ' +
        'alive as soon as those two are there',
    ).toBe(false);
  });
});
