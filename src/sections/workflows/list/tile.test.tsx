/* Kryterium 4 dla T-14: kafelek pokazuje wyłącznie fakty z pliku i nie zostawia pustych komórek.
 *
 * Słaba wersja to `expect(html).toContain('4 steps')`. Przechodzi dla napisu wpisanego na stałe
 * w komponencie i przechodzi obok kafelka, który pod spodem renderuje `used —`.
 *
 * Rozróżniają to trzy rzeczy. Przypadek jednego kroku — `1 step`, nie `1 steps` — wyklucza
 * zarówno napis na sztywno, jak i `${n} steps` bez odmiany. Cztery kroki o DWÓCH różnych
 * identyfikatorach agentów wykluczają liczenie kroków tam, gdzie ma się liczyć agentów.
 * I asercja negatywna na całą treść kafelka: `used 12×` oraz `~6 min` z makiety
 * (`docs/mockup/index.html:642-644`) wymagają historii biegów, której v1 nie ma, więc na
 * kafelku nie ma ich w żadnej postaci — ani jako `—`, ani jako `never`, ani jako
 * `not reported`. To jest ta sama komórka, którą poprzedni prototyp zostawił po sobie jako
 * `SPEND: not reported` (00-SYNTHESIS §6): miejsce na ekranie zajęte przez pole, które nigdy
 * nie będzie miało treści, tłumaczące się użytkownikowi z własnej pustki.
 *
 * Asercja negatywna biegnie po TREŚCI, nie po surowym HTML-u: nazwa klasy nie jest tekstem,
 * który ktokolwiek czyta, a `min-w-0` w klasie zapaliłoby czerwień, która nie mówi o niczym.
 * Trzy napisy, których żadna nazwa klasy nie zawiera (`used`, `never`, `not reported`),
 * sprawdzamy dodatkowo w surowym HTML-u — tam łapie się je także wtedy, gdy schowały się
 * w atrybucie.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { WorkflowFile } from './store';
import { WorkflowTile } from './tile';

/** Cztery kroki, dwa różne identyfikatory agentów: `4 steps`, `2 agents`. */
function research(): WorkflowFile {
  return {
    format: 1,
    id: 'wf-deep-research',
    name: 'Deep research',
    description: 'Six readers on six questions, then one writer folds them into one document.',
    steps: [
      { kind: 'agent', id: 's_read', name: 'Read the sources', agent: 'scout' },
      { kind: 'agent', id: 's_read_more', name: 'Read the code', agent: 'scout' },
      { kind: 'agent', id: 's_write', name: 'Write it up', agent: 'scribe' },
      { kind: 'agent', id: 's_polish', name: 'Tidy the wording', agent: 'scribe' },
    ],
    links: [{ from: 's_read', to: 's_write' }],
  };
}

/** Jeden krok, jeden agent: `1 step`, `1 agent`. Liczba pojedyncza, nie `1 steps`. */
function quickFix(): WorkflowFile {
  return {
    format: 1,
    id: 'wf-just-fix-it',
    name: 'Just fix it',
    description: 'One agent, no plan, no review. For a typo or a one-line change.',
    steps: [{ kind: 'agent', id: 's_fix', name: 'Fix it', agent: 'forge' }],
    links: [],
  };
}

/** Ten sam workflow bez opisu. Klucza nie ma wcale — nie jest pustym napisem. */
function undescribed(): WorkflowFile {
  return {
    format: 1,
    id: 'wf-nameless',
    name: 'Ship a feature',
    steps: [{ kind: 'agent', id: 's_fix', name: 'Fix it', agent: 'forge' }],
    links: [],
  };
}

/** To, co czyta człowiek: bez znaczników, z rozwiniętymi encjami, bez nadmiarowych odstępów. */
function visibleText(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, '')
    .replaceAll('&quot;', '"')
    .replaceAll('&#x27;', "'")
    .replaceAll('&lt;', '<')
    .replaceAll('&gt;', '>')
    .replaceAll('&amp;', '&')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Wszystko, co historia biegów przyniesie razem z T-06, i każde jej udawanie. */
const RUN_HISTORY = /used|min|never|—|not reported/i;

describe('a workflow tile shows what is in the file and leaves no empty cells', () => {
  it('counts the steps, and counts the agents as the different ones among them', () => {
    const markup = renderToStaticMarkup(<WorkflowTile wf={research()} />);
    const text = visibleText(markup);

    expect(text, 'four steps in the file, four steps on the tile').toContain('4 steps');
    expect(
      text,
      'and two agents, because two of the four names repeat. Counting the steps here reads ' +
        'the same on this workflow until somebody looks at a workflow where it does not',
    ).toContain('2 agents');
    expect(text, 'so the agent count is not the step count wearing another label').not.toContain(
      '4 agents',
    );
    expect(text, 'the one sentence from the file is what describes it').toContain(
      'Six readers on six questions',
    );
    expect(text, 'and the name it was saved under').toContain('Deep research');
  });

  it('says 1 step and 1 agent, not 1 steps and 1 agents', () => {
    const text = visibleText(renderToStaticMarkup(<WorkflowTile wf={quickFix()} />));

    expect(text, 'one step').toContain('1 step');
    expect(
      text,
      'and the word follows the number. `${n} steps` reads fine at four and wrong at one, ' +
        'and a hard-coded "4 steps" passes every count assertion made on a four-step workflow',
    ).not.toContain('1 steps');
    expect(text, 'one agent').toContain('1 agent');
    expect(text, 'same rule, same reason').not.toContain('1 agents');
  });

  it('leaves the description out entirely when the file has none', () => {
    const markup = renderToStaticMarkup(<WorkflowTile wf={undescribed()} />);

    expect(
      markup,
      'no empty paragraph. An always-rendered description holds a line of the tile open for ' +
        'text that is not there, and every tile without one is a row of nothing',
    ).not.toMatch(/<p[^>]*>\s*<\/p>/);
    expect(visibleText(markup), 'the rest of the tile is still there').toContain('Ship a feature');
    expect(visibleText(markup), 'including the counts').toContain('1 step');
  });

  it('shows nothing about how often or how long, because there are no runs to read yet', () => {
    for (const workflow of [research(), quickFix(), undescribed()]) {
      const markup = renderToStaticMarkup(<WorkflowTile wf={workflow} />);

      expect(
        visibleText(markup),
        'the mockup shows `used 12×` and `~6 min`, and both come from run history that v1 does ' +
          'not have. They arrive with T-06, together with the data — never first as an empty ' +
          'cell explaining itself. Tile of: ' +
          workflow.name,
      ).not.toMatch(RUN_HISTORY);
      expect(
        markup,
        'and not hidden in an attribute either. These three words cannot occur in a class name, ' +
          'so the raw markup can be asked about them directly. Tile of: ' +
          workflow.name,
      ).not.toMatch(/used|never|not reported/i);
    }
  });
});
