/* AC-3 dla T-93: panel kroku pokazuje, co TEN folder ma do pożyczenia — i zapisuje wybór
 * w kroku.
 *
 * Wiersz jest jedynym miejscem, w którym `borrow` powstaje. Bez niego pole istnieje w schemacie,
 * w Ruście i w pliku na dysku, a jedyną drogą do niego jest edytor tekstu — czyli dokładnie ten
 * stan, w którym `Thinking` przeżyło pięć wydań jako ustawienie bez wołającego.
 *
 * CZTERY SŁABE WERSJE TEGO KRYTERIUM
 *
 * Pierwsza: `expect(html).toContain('Borrow from this project')`. Przechodzi dla nagłówka nad
 * pustką i dla listy, która pokazuje trzy pozycje z dziesięciu. Rozróżnia to asercja na KAŻDEJ
 * nazwie z każdej z trzech półek osobno.
 *
 * Druga: sam render. Kontrolka, która się rysuje i niczego nie zapisuje, jest niezmiennikiem 16
 * i wygląda dokładnie jak działająca. Rozróżnia to wykonanie tej samej funkcji, którą woła
 * `onChange` pola wyboru — w obie strony, bo zaznaczenie bez odznaczenia zostawia człowieka
 * z wyborem, którego nie da się cofnąć z okna.
 *
 * Trzecia: cichy render pustego wiersza dla folderu, który nie ma czego pożyczyć. „Nie ma"
 * znaczy nie ma: nagłówek nad zerem pozycji obiecuje funkcję, po której nikt nie przyjdzie,
 * a przy folderze bez `.claude/` jest to WIĘKSZOŚĆ folderów.
 *
 * Czwarta, najcichsza: ciche zdjęcie nazwy, której w folderze już nie ma. Wtedy krok, który
 * pożyczał rolę, przestaje ją pożyczać przy pierwszym otwarciu panelu w innym projekcie — i nic
 * o tym nie mówi. Dlatego zapisana nazwa spoza tego, co znalazł skan, ma się pokazać z „not in
 * this folder", a nie zniknąć.
 *
 * DLACZEGO ŚCIEŻKA DANYCH JEST TU SĄDZONA OSOBNO. Lista bez komendy jest listą wymyśloną przez
 * front, a `renderToStaticMarkup` nie uruchamia efektów, więc żadna asercja o markupie nie
 * dotknie drogi, którą ta lista przyjeżdża. Ostatni przypadek wykonuje więc krawędź naprawdę,
 * na atrapie `@tauri-apps/api/core` — tak samo, jak robi to lustro komend.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../../state/agents';
import type { AgentStep, Borrow, HostMaterial } from '../../../state/workflows';
import { BorrowRow, ticked } from './borrow-row';
import { PanelForStep } from './panel';

/* Atrapa transportu, podniesiona razem z `vi.mock`. Ta droga nie mierzy odpowiedzi Rusta, tylko
 * to, co w jego stronę pojechało. */
const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((..._sent: unknown[]) =>
    Promise.resolve({ skills: [], learnings: [], subagents: [] }),
  ),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const FOLDER = '/Users/someone/Projects/urc-monorepo';

/** Trzy półki, każda z dwiema pozycjami — po jednej wybranej i jednej nie. */
const MATERIAL: HostMaterial = {
  skills: ['code-review', 'deep-research'],
  learnings: ['backend-dev', 'frontend'],
  subagents: ['release-engineer', 'learnings-extractor'],
};

function step(borrow?: Borrow): AgentStep {
  return {
    kind: 'agent',
    id: 's_build',
    name: 'Build',
    agent: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f77',
    overrides: {},
    copies: 1,
    instructions: 'Fix the failing parser tests.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y: 168 },
    ...(borrow === undefined ? {} : { borrow }),
  };
}

function agent(): Agent {
  return {
    schema: 1,
    id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f77',
    name: 'Hand',
    summary: 'Does the work',
    color: 'moss',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'work-freely',
    giveUpAfterMinutes: 20,
    writeResultsTo: '',
    instructions: 'Do the work.',
    tools: 'everything',
    skills: [],
    connections: [],
    reachesTheWeb: false,
  } as unknown as Agent;
}

const nothing = (): void => undefined;

describe('the panel says what this folder has to lend', () => {
  it('lists every skill, every role and every subagent the folder holds', () => {
    const html = renderToStaticMarkup(
      <BorrowRow material={MATERIAL} value={{}} onChoose={nothing} />,
    );

    expect(
      html,
      'the row that lends this project to a step has no heading, so nothing on screen says ' +
        'what these boxes are about',
    ).toContain('Borrow from this project');

    for (const name of [...MATERIAL.skills, ...MATERIAL.learnings, ...MATERIAL.subagents]) {
      expect(
        html,
        'this folder holds ' +
          name +
          ' and the row does not offer it. A list that shows some of what is there is worse ' +
          'than none: what is missing looks like what the folder does not have',
      ).toContain(name);
    }

    expect(
      (html.match(/type="checkbox"/g) ?? []).length,
      'every entry needs a box of its own. Six entries and fewer boxes means at least one of ' +
        'them cannot be picked, and nothing on screen would say which',
    ).toBe(6);
  });

  it('is not there at all for a folder with nothing to lend', () => {
    const empty = renderToStaticMarkup(
      <BorrowRow
        material={{ skills: [], learnings: [], subagents: [] }}
        value={{}}
        onChoose={nothing}
      />,
    );
    expect(
      empty,
      'a folder that holds nothing still drew the row. A heading over zero entries promises ' +
        'something nobody will ever come and switch on, and a folder without a .claude ' +
        'directory is most folders',
    ).toBe('');
  });

  it('shows a saved name the folder no longer holds instead of dropping it', () => {
    const html = renderToStaticMarkup(
      <BorrowRow
        material={MATERIAL}
        value={{ skills: ['gone-from-here'], learnings: 'backend-dev' }}
        onChoose={nothing}
      />,
    );

    expect(
      html,
      'the step borrows gone-from-here and this folder does not hold it. Dropping it quietly ' +
        'means the step stops borrowing it the first time somebody opens this panel in ' +
        'another project, and nothing says so',
    ).toContain('gone-from-here');
    expect(
      html,
      'the name that is no longer in this folder is shown as though it were still there',
    ).toContain('not in this folder');
    expect(
      html.indexOf('not in this folder'),
      'a name this folder really does hold was labelled as missing too',
    ).toBeGreaterThan(html.indexOf('gone-from-here'));
  });

  it('writes a ticked box into the step, and an unticked one back out', () => {
    const one = ticked({}, 'skills', 'code-review');
    expect(one.skills, 'ticking a skill did not put it on the step').toEqual(['code-review']);

    const two = ticked(one, 'skills', 'deep-research');
    expect(
      two.skills,
      'the second tick replaced the first instead of adding to it, so only one skill can ever ' +
        'be borrowed',
    ).toEqual(['code-review', 'deep-research']);

    const back = ticked(two, 'skills', 'code-review');
    expect(
      back.skills,
      'unticking left the skill on the step, so a choice made by mistake cannot be taken back ' +
        'from this window',
    ).toEqual(['deep-research']);

    const role = ticked({}, 'learnings', 'backend-dev');
    expect(role.learnings, 'ticking a role did not put it on the step').toBe('backend-dev');
    expect(
      ticked(role, 'learnings', 'backend-dev').learnings,
      'unticking the role left it on the step',
    ).toBeUndefined();

    const sub = ticked({}, 'subagents', 'release-engineer');
    expect(sub.agent, 'ticking a subagent did not put it on the step').toBe('release-engineer');
  });

  it('is mounted in the panel of an agent step, with what that step already borrows', () => {
    const html = renderToStaticMarkup(
      <PanelForStep
        step={step({ skills: ['code-review'] })}
        agents={[agent()]}
        skills={[]}
        onChooseAgent={nothing}
        onCreateAgent={nothing}
        onEdit={nothing}
        onEditStep={nothing}
        onEditCheckpoint={nothing}
        onEditServe={nothing}
        onReset={nothing}
        onChooseSkills={nothing}
        wayBack={null}
        onEditWayBack={nothing}
      />,
    );

    expect(
      html,
      'the row exists as a component and the panel does not mount it, so nothing a person can ' +
        'click ever reaches it',
    ).toContain('Borrow from this project');
    expect(
      html,
      'the panel mounted the row and it does not carry what this step already borrows',
    ).toContain('code-review');
  });

  it('asks Rust for this folder, by the one command name both sides agree on', async () => {
    const { listHostMaterial } = await import('../io');
    invoked.mockClear();

    await listHostMaterial(FOLDER);

    expect(
      invoked.mock.calls.length,
      'the list this row shows never reached Rust at all, so it is a list the window made up',
    ).toBe(1);
    const sent = invoked.mock.calls.at(0);
    expect(sent?.at(0), 'the row asked Rust for the wrong thing').toBe('list_host_material');
    expect(
      sent?.at(1),
      'the folder has to travel with the question. Without it Rust would answer about some ' +
        'other folder, and the row would offer things this project does not have',
    ).toEqual({ folder: FOLDER });
  });
});
