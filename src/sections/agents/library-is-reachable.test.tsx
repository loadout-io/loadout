/* Sekcja Agents ma być OSIĄGALNA: agent daje się otworzyć, odmowa jest widoczna, a liczba
 * „used in N workflows" jest policzona, nie narysowana.
 *
 * DLACZEGO TEN PLIK ISTNIEJE. 440 testów było zielonych, a `~/.loadout/agents` nie istniał na
 * maszynie właściciela. Każda kontrolka w tej sekcji REAGOWAŁA, żadna nie miała skutku, którego
 * cokolwiek by pilnowało: zapis jechał bez `catch`, kafelek był `<li>` bez handlera, a `delete`
 * i `duplicate` nie miały produkcyjnego wołającego, choć `delete_agent` był zarejestrowany po
 * stronie Rusta. To wszystko są zdania o MARKUPIE i o funkcjach czystych, więc dają się
 * sprawdzić bez jsdom — a właśnie brak takich zdań pozwolił defektom przeżyć.
 *
 * CZEGO TU NIE MA I DLACZEGO. „Klikam kafelek i otwiera się panel" jest w tym repo
 * niesprawdzalne: `renderToStaticMarkup` nigdy nie odpala `onClick`, a jsdom nie ma i nie
 * będzie (`package.json` jest zablokowany). Sprawdzamy więc dwie połowy osobno i mówimy
 * wprost, że to dwie połowy: (1) element, który człowiek klika, JEST przyciskiem i niesie
 * identyfikator agenta — a `<li>` bez handlera tę asercję przewraca; (2) funkcja, którą ten
 * handler woła, robi to, co ma, i to jest sprawdzone na magazynie w
 * `src/state/agents-save.test.ts`. Sama obecność `<button>` nie jest dowodem, że cokolwiek
 * się stanie, i ten komentarz istnieje po to, żeby nikt jej za dowód nie wziął.
 */
import { readFileSync } from 'node:fs';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Agent, AgentsIo } from '../../state/agents';
import { createAgentsStore } from '../../state/agents';
import type { AgentStep, WorkflowFile } from '../../state/workflows';
import { AgentForm } from './agent-form';
import AgentsScreen from './index';
import { countUsage, usageSays, usedIn } from './usage';
import type { WorkflowEntry } from '../workflows/list/store';

/* Makieta jest jedyną wyrocznią wyglądu (commit 6bc74b6), a `theme.css` jest lustrem DESIGN.md.
 * Oba są CZYTANE w tym samym biegu testu: liczba przepisana z palca przechodzi także wtedy, gdy
 * makieta mówi co innego, i to jest najczęstszy sposób, w jaki test o wyglądzie staje się
 * pieczątką. */
const MOCKUP = readFileSync(new URL('../../../docs/mockup/index.html', import.meta.url), 'utf8');
const THEME = readFileSync(new URL('../../styles/theme.css', import.meta.url), 'utf8');
const DESIGN = readFileSync(new URL('../../../docs/design/DESIGN.md', import.meta.url), 'utf8');

function agent(over: Partial<Agent> = {}): Agent {
  return {
    schema: 1,
    id: 'a-1',
    name: 'Forge',
    summary: 'Writes code',
    color: 'clay',
    instructions: 'Write the smallest change that makes the checks pass.',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'deep',
    fileAccess: 'work-freely',
    giveUpAfterMinutes: 20,
    tools: 'everything',
    skills: [],
    connections: [],
    writeResultsTo: '',
    ...over,
  };
}

/** Atrapa dysku: `list` oddaje to, co zasialiśmy; nic tu nie zapisuje ani nie usuwa. */
function ioWith(agents: readonly Agent[]): AgentsIo {
  return {
    list: () => Promise.resolve([...agents]),
    newId: () => Promise.resolve('a-new'),
    save: () => Promise.resolve(),
    remove: () => Promise.resolve(),
  };
}

async function screenOf(
  agents: readonly Agent[],
  usage?: Record<string, number>,
  opened?: Agent,
): Promise<string> {
  const store = createAgentsStore(ioWith(agents));
  await store.getState().load();
  if (opened !== undefined) {
    return renderToStaticMarkup(
      <AgentsScreen store={store} usage={usage ?? null} opened={opened} />,
    );
  }
  /* Bez propsu ekran czytałby prawdziwy katalog workflow przez `invoke`, czyli przez transport,
   * którego w teście nie ma — a `useEffect` w statycznym renderze i tak się nie odpala, więc
   * `usage` zostaje `null` i wiersz `used in …` nie istnieje. Tego właśnie chce jedna z asercji
   * niżej; `null` podany jawnie znaczy dokładnie to samo i mówi to na głos. */
  return renderToStaticMarkup(<AgentsScreen store={store} usage={usage ?? null} />);
}

/** Znacznik otwierający element, który niesie ten atrybut. */
function tag(markup: string, attribute: string): string {
  const at = markup.indexOf(attribute);
  if (at < 0) return '';
  const open = markup.lastIndexOf('<', at);
  const close = markup.indexOf('>', at);
  return close < 0 ? '' : markup.slice(open, close + 1);
}

function step(over: Partial<AgentStep> = {}): AgentStep {
  return {
    kind: 'agent',
    id: 's-1',
    name: 'Build',
    agent: 'a-1',
    overrides: {},
    copies: 1,
    instructions: '',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 0, y: 0 },
    ...over,
  };
}

function entry(path: string, steps: AgentStep[]): WorkflowEntry {
  const workflow: WorkflowFile = { format: 1, id: path, name: path, steps, links: [] };
  return { path, workflow };
}

describe('a saved agent can be opened, and a refused save says so', () => {
  it('makes every tile a control that carries the agent it opens', async () => {
    const markup = await screenOf([agent(), agent({ id: 'a-2', name: 'Needle', color: 'slate' })]);

    for (const id of ['a-1', 'a-2']) {
      const opener = tag(markup, `data-agent="${id}"`);

      expect(
        opener,
        'the element a person clicks to open agent ' +
          id +
          ' has to exist. Until 2026-08-18 this was an <li> with no handler at all, so an agent ' +
          'saved once stayed on the list forever, typos and all',
      ).not.toBe('');
      expect(
        opener.startsWith('<button'),
        'and it has to be a button, the way the mockup draws it (`<button class="tile">`). ' +
          'A <li> or a <div> here is a tile that cannot be reached by keyboard and, in this ' +
          'section, could not be reached at all',
      ).toBe(true);
    }
  });

  it('shows the sentence the disk wrote, and nothing when the disk said nothing', async () => {
    const quiet = await screenOf([agent()]);
    expect(
      quiet,
      'a section with nothing wrong has no alert on it. Without this line the assertion below ' +
        'also passes for a screen that shows a refusal banner permanently',
    ).not.toContain('data-refusal');

    const said = 'the agents folder is read-only, so Forge was not written';
    const store = createAgentsStore({
      list: () => Promise.resolve([]),
      newId: () => Promise.resolve('a-new'),
      save: () => Promise.reject(said),
      remove: () => Promise.resolve(),
    });
    await store.getState().save(agent({ id: '' }));

    const loud = renderToStaticMarkup(<AgentsScreen store={store} />);

    expect(
      loud,
      'the refusal has to be ON THE SCREEN, not merely in the state. A field nobody renders is ' +
        'the same silence as no field at all — and that silence is why the agents folder never ' +
        'came into being',
    ).toContain(said);
    expect(
      tag(loud, 'data-refusal'),
      'and it is announced, so it is not a sentence that only the person looking at that corner ' +
        'of the panel will notice',
    ).toContain('role="alert"');
  });

  it('never reaches for window.confirm, because that dialog takes the whole window', () => {
    const source = readFileSync(new URL('./index.tsx', import.meta.url), 'utf8');

    expect(
      /window\.confirm\s*\(/.test(source),
      'a browser dialog blocks the webview and there is nothing in a Tauri window to unblock it ' +
        'with. Deleting asks in a real render or it does not ask',
    ).toBe(false);
  });
});

describe('the identity square is the mockup size and never a state colour', () => {
  it('is 22px, the number the mockup states, and gets there through the spacing step', async () => {
    /* Oczekiwana liczba jest CZYTANA z makiety w tym samym biegu. Wpisana z palca `22`
     * przechodziłaby także wtedy, gdyby makieta mówiła 14 — a DESIGN.md mówi w jednym miejscu
     * dokładnie to (linia 243), więc ta pomyłka nie jest hipotetyczna. */
    const wanted = /\.sqid\{width:(\d+)px;height:(\d+)px/.exec(MOCKUP);
    expect(wanted, 'the mockup has to state the size of `.sqid`; it no longer does').not.toBeNull();
    expect(wanted?.[1], 'the square is a SQUARE').toBe(wanted?.[2]);

    const base = /--spacing:\s*(\d+)px/.exec(THEME);
    expect(
      base,
      'theme.css has to state the spacing base; without it the maths below is air',
    ).not.toBeNull();

    const markup = await screenOf([agent()]);
    const square = tag(markup, 'data-identity=');
    expect(square, 'every tile carries an identity square').not.toBe('');

    const step = /\bsize-(\d+(?:\.\d+)?)\b/.exec(square);
    expect(
      step,
      'the size comes from a spacing step, not from a literal. `size-[22px]` would be the same ' +
        'escape hatch written as a class, and the check for literals refuses it all the same',
    ).not.toBeNull();

    expect(
      Number(step?.[1] ?? 0) * Number(base?.[1] ?? 0),
      'the rendered square is the size the mockup states. DESIGN.md contradicts itself here — ' +
        '22px in line 127, 14px in line 243 — and the mockup wins',
    ).toBe(Number(wanted?.[1] ?? 0));
  });

  it('paints identity with the muted --id-* values, never with a state colour', async () => {
    const markup = await screenOf([
      agent({ id: 'a-1', color: 'clay' }),
      agent({ id: 'a-2', color: 'rose' }),
    ]);

    /* Makieta daje kwadratowi Forge'a `var(--id-3)`, a `clay` jest trzecim wariantem unii
     * `Color`. Ta para jest czytana z makiety, nie założona. */
    expect(
      /class="sqid" style="background:var\(--id-3\)">F</.test(MOCKUP),
      'the mockup has to still paint the Forge square with --id-3; the mapping below leans on it',
    ).toBe(true);
    expect(tag(markup, 'data-identity="clay"'), 'clay is the third identity value').toContain(
      'bg-id-3',
    );
    expect(
      tag(markup, 'data-identity="rose"'),
      'and a different colour has to reach a different one. Without this the assertion above ' +
        'also passes for a screen that paints every square the same',
    ).toContain('bg-id-5');

    for (const state of ['bg-accent', 'bg-attend', 'bg-fail', 'bg-human']) {
      expect(
        tag(markup, 'data-identity="clay"'),
        'identity is never a state colour: the four saturated ones mean "now", "your turn", ' +
          '"broken" and "a person did this" (DESIGN §3). The reference poprzedni prototyp gave the agent ' +
          'Forge the exact hex of "needs attention"',
      ).not.toContain(state);
    }
  });
});

describe('"used in 3 workflows" is counted from the workflow files, or not shown at all', () => {
  it('counts FILES that name the agent, not steps', () => {
    const counted = countUsage([
      /* Ten sam agent w dwóch krokach jednego pliku. To jest JEDEN workflow. */
      entry('ship.json', [step({ id: 's-1' }), step({ id: 's-2' })]),
      entry('check.json', [step({ id: 's-3' })]),
      entry('other.json', [step({ id: 's-4', agent: 'a-2' })]),
    ]);

    expect(
      usedIn(counted, 'a-1'),
      'two steps in one file are one workflow. Counting steps prints "used in 3 workflows" and ' +
        'sends the person looking for a third file that does not exist',
    ).toBe(2);
    expect(usedIn(counted, 'a-2'), 'and the other agent is used by exactly its one file').toBe(1);
    expect(
      usedIn(counted, 'a-3'),
      'an agent no workflow names is used in zero, and zero is a number we know — not a guess',
    ).toBe(0);
  });

  it('says workflow in the singular when there is one, exactly as the mockup words it', () => {
    expect(
      /used in 3 workflows/.test(MOCKUP) && /used in 1 workflow</.test(MOCKUP),
      'the wording under test is the mockup wording, plural and singular both. If the mockup ' +
        'changed, this test is the thing that has to change first',
    ).toBe(true);
    expect(usageSays(3)).toBe('used in 3 workflows');
    expect(
      usageSays(1),
      '"used in 1 workflows" is the kind of detail after which a person stops believing the rest ' +
        'of the numbers on the screen',
    ).toBe('used in 1 workflow');
  });

  it('draws the row when the catalogue was read, and stays silent when it was not', async () => {
    const counted = await screenOf([agent()], { 'a-1': 2 });

    expect(
      counted,
      'the row is the mockup row and it carries the number that was actually counted',
    ).toContain('used in 2 workflows');

    const unread = await screenOf([agent()]);
    expect(
      /used in \d+ workflow/.test(unread),
      'with the workflow catalogue unread the row is ABSENT — not "used in 0 workflows". UI does ' +
        'not draw relations that are not in the data (invariant 17), and a zero shown because a ' +
        'read never finished is a false sentence, not a cautious one',
    ).toBe(false);
  });
});

describe('the form offers every agent app the runtime can drive', () => {
  it('offers Codex as a pickable agent app', () => {
    const markup = renderToStaticMarkup(
      <AgentForm
        value={agent()}
        expanded={false}
        onChange={() => undefined}
        onToggleMore={() => undefined}
        onSave={() => undefined}
      />,
    );

    const codex = /<option value="codex"([^>]*)>([^<]*)</.exec(markup);
    expect(codex, 'Codex is missing even though the runtime has a real CodexDriver').not.toBe(null);
    expect(
      /\bdisabled\b/.test(codex?.[1] ?? ''),
      'the runtime maps Codex to CodexDriver, so greying it out would make the form deny a ' +
        'capability the application really has',
    ).toBe(false);
    expect(codex?.[2] ?? '', 'a runnable agent app is named without the old warning suffix').toBe(
      'Codex',
    );

    const claude = /<option value="claude-code"([^>]*)>/.exec(markup);
    expect(
      /\bdisabled\b/.test(claude?.[1] ?? ''),
      'Claude Code remains pickable. Without this line the assertion above also ' +
        'passes for a form that greyed out both of them',
    ).toBe(false);
  });

  it('does not show the retired no-driver warning for either runnable app', () => {
    const of = (value: Agent): string =>
      renderToStaticMarkup(
        <AgentForm
          value={value}
          expanded={false}
          onChange={() => undefined}
          onToggleMore={() => undefined}
          onSave={() => undefined}
        />,
      );

    expect(
      of(agent({ runsWith: 'codex' })),
      'the runtime has a real CodexDriver, so warning that the step will not run is a lie',
    ).not.toContain('data-no-driver');
    expect(
      of(agent({ runsWith: 'claude-code' })),
      'the retired warning stays absent for Claude Code too',
    ).not.toContain('data-no-driver');
  });
});

describe('the form explains why Save is blocked', () => {
  it('says WHICH field is missing before it says nothing at all', () => {
    const blocked = (over: Partial<Agent>): string =>
      renderToStaticMarkup(
        <AgentForm
          value={agent(over)}
          expanded={false}
          onChange={() => undefined}
          onToggleMore={() => undefined}
          onSave={() => undefined}
        />,
      );

    expect(
      blocked({ instructions: '' }),
      'the disabled Save has to be explained. `grep required|aria-required` used to give zero ' +
        'hits in these controls, so a person saw a live-looking button that did nothing',
    ).toContain('Fill in Instructions to save this agent.');
    expect(blocked({ name: '' }), 'and the empty field is named, not the other one').toContain(
      'Fill in Name to save this agent.',
    );
    expect(
      blocked({}),
      'a complete agent gets no such sentence. A note that never goes away explains nothing',
    ).not.toContain('data-save-blocked');
  });
});

describe('the panel of a saved agent can change it, copy it and delete it', () => {
  it('opens the agent that was picked, with its own values in the fields', async () => {
    const forge = agent({ name: 'Forge', instructions: 'Keep the public API unless told.' });
    const markup = await screenOf([forge], undefined, forge);

    expect(
      markup,
      'the panel names the agent it holds, not "New agent". Until 2026-08-18 the panel mounted ' +
        'ONLY for a fresh draft, so a saved agent could never be looked at again',
    ).toContain('>Forge<');
    expect(
      markup,
      'and it holds that agent\u2019s own instructions, so the person edits what is on disk ' +
        'rather than a blank',
    ).toContain('Keep the public API unless told.');
    expect(
      markup,
      'a saved agent is not a new one, so the new-agent heading has no business here',
    ).not.toContain('New agent');
  });

  it('offers Duplicate and Delete for a saved agent, and neither for a fresh draft', async () => {
    const forge = agent();
    const saved = await screenOf([forge], undefined, forge);

    expect(
      saved,
      'duplicating had NO production caller at all — the store action existed and nothing on ' +
        'screen reached it',
    ).toContain('data-duplicate');
    expect(
      saved,
      'and neither did deleting, while `delete_agent` sat registered on the Rust side, ' +
        'unreachable from the window',
    ).toContain('data-delete');

    const fresh = await screenOf([forge], undefined, agent({ id: '', name: '', instructions: '' }));
    expect(
      fresh,
      'a draft that was never written has no file to copy or delete. A control whose handler ' +
        'cannot have an effect is worse than no control (invariant 16)',
    ).not.toContain('data-delete');
  });

  it('draws Delete as button-danger: the fail edge, and no fill', async () => {
    /* Regułę czytamy z DESIGN §6 w tym samym biegu. Wpisana z palca lista klas przechodzi
     * także wtedy, gdy dokument mówi co innego — a to jest dokładnie ten rozjazd, którego
     * quick-tokens.sh pilnuje po stronie kolorów i nie umie pilnować po stronie komponentów. */
    const rule = /### `button-danger`\n([^\n]*\n[^\n]*)/.exec(DESIGN);
    expect(
      rule,
      'DESIGN.md has to still describe button-danger; the assertions lean on it',
    ).not.toBeNull();
    const said = rule?.[1] ?? '';
    expect(said, 'the rule names the fail edge as the border').toContain('--fail-edge');
    expect(said, 'and says there is no fill').toContain('Bez wype');

    const forge = agent();
    const markup = await screenOf([forge], undefined, forge);
    const button = tag(markup, 'data-delete=');

    expect(button, 'the delete control has to be on screen').not.toBe('');
    expect(
      button,
      'a destructive action is recognisable by its edge, not by a block of colour',
    ).toContain('border-fail-edge');
    expect(
      button,
      'and it carries no fill: a filled red button is the loudest thing on the screen, which is ' +
        'the opposite of what DESIGN \u00a76 asks for',
    ).not.toContain('bg-fail');
  });
});
