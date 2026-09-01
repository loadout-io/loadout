/* AC-5 dla T-90: `More settings` dostaje wiersz przelotki, a ten wiersz naprawdę pisze do
 * definicji agenta i naprawdę ją odczytuje z powrotem.
 *
 * Słaba wersja tego kryterium to `expect(html).toContain('Extra options')`. Przechodzi dla
 * napisu nad kontrolką, której `onChange` nie ma — czyli dla dokładnie tej wady, którą to
 * zadanie zamyka: pola, które wygląda jak ustawienie, a niczego nie ustawia (niezmiennik 16).
 * Dlatego niżej stoi PODRÓŻ W OBIE STRONY przez PRAWDZIWY komponent: tekst wchodzi handlerem
 * z drzewa, które zwraca `MoreSettings`, a wraca z drugiego renderu tego samego komponentu.
 *
 * Drzewo elementów, a nie `jsdom`: w repo nie ma ani `jsdom`, ani `@testing-library/react`
 * (`package.json` jest na liście DENIED w `checks/quick-scope.sh`). Wzorzec jest cudzy
 * i sprawdzony — `src/sections/triggers/setup-actions-are-real.test.tsx` woła handlery wyjęte
 * z prawdziwego formularza. Statyczny markup zostaje tam, gdzie pytamy o to, co widać.
 *
 * Klucz w `vendorOptions` to `claude` i `codex`, a nie `claude-code`: tak nazywa je plik
 * agenta po stronie Rusta (`library/agents.rs`, `vendor_args_filtered(&agent, "claude")`).
 * Wpisanie tu drugiej nazwy dałoby przelotkę, która zapisuje się na ekranie i nie dojeżdża
 * do żadnego z dwóch vendorów.
 *
 * Odmowa zapisu jedzie ISTNIEJĄCYM nośnikiem — `missingForSave` plus zdanie pod przyciskiem
 * Save — bo ten warunek ma już dwóch wołających: formularz i `save` w magazynie. Trzecia
 * kopia reguły znaczyłaby przycisk, który budzi się przy wierszu bez wartości, i zapis, który
 * go dalej nie przyjmuje (niezmiennik 13).
 */
import { Children, isValidElement } from 'react';
import type { ReactElement, ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Agent, AgentsIo } from '../../state/agents';
import { createAgentsStore, missingForSave } from '../../state/agents';
import { AgentForm } from './agent-form';
import { MoreSettings } from './more-settings';

/** Ten sam znacznik, co czterech wierszy obok: `data-field` w `more-settings.tsx`. */
const FIELD = 'vendorOptions';

/* Pary spoza obu list zarezerwowanych (`workflow/check.rs`) i bez podniesienia diala — wiersz
 * odrzucony przez politykę mierzyłby politykę, a nie podróż w obie strony. */
const CLAUDE_TEXT = '--fallback-model sonnet\n--add-dir /work/shared';
const CLAUDE_LINES = ['--fallback-model sonnet', '--add-dir /work/shared'];
const CLAUDE_KEPT = { '--fallback-model': 'sonnet', '--add-dir': '/work/shared' };

const CODEX_TEXT = 'model_verbosity=high\nfile_opener=vscode';
const CODEX_LINES = ['model_verbosity=high', 'file_opener=vscode'];
const CODEX_KEPT = { model_verbosity: 'high', file_opener: 'vscode' };

/** Wiersz z nazwą i bez wartości — jedyna forma, którą to pole odrzuca. */
const HALF_TEXT = '--fallback-model sonnet\n--add-dir';
const HALF_LINE = '--add-dir';

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
  reachesTheWeb: false,
  skills: [],
  connections: [],
  writeResultsTo: 'handoffs/build.md',
};

const CODIE: Agent = { ...FORGE, runsWith: 'codex' };

function noop(): void {
  /* sterowany formularz: w statycznym renderze nic tego nie woła */
}

interface ControlProps {
  readonly 'data-field'?: string;
  readonly value?: unknown;
  readonly children?: ReactNode;
  readonly onChange?: (event: { readonly target: { readonly value: string } }) => unknown;
}

/** Pierwszy element niosący ten `data-field`, albo `null`, kiedy takiego nie ma. */
function findField(node: ReactNode, field: string): ReactElement<ControlProps> | null {
  if (!isValidElement(node)) return null;
  const props = node.props as ControlProps;
  if (props['data-field'] === field) return node as ReactElement<ControlProps>;
  for (const child of Children.toArray(props.children)) {
    const found = findField(child, field);
    if (found !== null) return found;
  }
  return null;
}

function markupOf(value: Agent): string {
  return renderToStaticMarkup(<MoreSettings value={value} onChange={noop} />);
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

/** Atrybuty przycisku o tym napisie, albo `null`, kiedy takiego przycisku nie ma. */
function buttonAttributes(html: string, name: string): string | null {
  for (const hit of html.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)) {
    if (plain(hit[2] ?? '') === name) return hit[1] ?? '';
  }
  return null;
}

/** Wiersze, które to pole pokazuje. Puste linie nie są wpisami. */
function linesOf(text: string): string[] {
  return text
    .split('\n')
    .map((one) => one.trim())
    .filter((one) => one.length > 0);
}

/** Co pole pokazuje dla tego agenta — czyli co człowiek zobaczy, otwierając go ponownie. */
function shown(value: Agent): string {
  const found = findField(MoreSettings({ value, onChange: noop }), FIELD);
  expect(
    found,
    'More settings carries no extra-options control at all, so there is nothing to read back. ' +
      'A setting a person writes and the form forgets is the same to them as one that was ' +
      'never saved',
  ).not.toBeNull();
  const text = found?.props.value;
  expect(
    typeof text,
    'the extra-options control has to carry the text it shows. A control whose value comes ' +
      'from nowhere shows an empty box over a file that is not empty',
  ).toBe('string');
  return typeof text === 'string' ? text : '';
}

/** Wpisuje ten tekst w PRAWDZIWE pole i oddaje agenta, którego formularz z tego zrobił. */
function typed(value: Agent, text: string): Agent {
  const handed: Agent[] = [];
  const tree = MoreSettings({
    value,
    onChange: (one) => {
      handed.push(one);
    },
  });
  const found = findField(tree, FIELD);
  expect(
    found,
    'More settings carries no extra-options control, so there is nothing to type into. This is ' +
      'the whole of what this criterion asks for',
  ).not.toBeNull();
  expect(
    found?.props.onChange,
    'the extra-options control has no change handler, so every letter typed into it is lost. ' +
      'A control with nothing behind it is worse than no control (invariant 16)',
  ).toBeTypeOf('function');
  found?.props.onChange?.({ target: { value: text } });
  expect(
    handed.length,
    'typing into the extra-options control handed nothing back to the form. The form is ' +
      'controlled from above, so a handler that keeps the text to itself keeps it nowhere',
  ).toBe(1);
  return handed[0] ?? value;
}

function ioThatRecords(saved: Agent[]): AgentsIo {
  return {
    list: async () => [],
    newId: async () => '019897b4-8f3a-7c21-9d44-0b6a1e2c5f78',
    save: async (agent: Agent) => {
      saved.push(agent);
      return 'after-the-save';
    },
    remove: async () => undefined,
  };
}

/* Atrapa, która ZACHOWUJE SIĘ JAK DYSK: `save` odkłada plik, `list` oddaje to, co na nim leży.
 *
 * 2026-08-24 — DODANA, i bez niej „podróż w obie strony" odbywała się w całości w pamięci tego
 * pliku: `typed()` oddaje obiekt, a `shown()` renderuje TEN SAM obiekt, więc obie strony podróży
 * to jedna wartość, która nigdzie nie pojechała. Atrapa obok (`ioThatRecords`) oddaje z `list`
 * pustą listę i jej `save` nie jest wołany ani razu po udanej drodze — tamta służy wyłącznie
 * odmowie. TASK.md mówi „wraca na ekran PO PONOWNYM OTWARCIU", a otwarcie na nowo czyta plik.
 *
 * Kopiuje przy obu krawędziach z rozmysłem: atrapa oddająca tę samą referencję, którą przyjęła,
 * przechodzi każdą asercję poniżej także wtedy, gdy zapis nie zapisał niczego. */
function diskThatKeeps(files: Agent[]): AgentsIo {
  return {
    list: async () => files.map((one) => structuredClone(one)),
    newId: async () => '019897b4-8f3a-7c21-9d44-0b6a1e2c5f78',
    save: async (agent: Agent) => {
      const at = files.findIndex((one) => one.id === agent.id);
      const copy = structuredClone(agent);
      if (at === -1) files.push(copy);
      else files[at] = copy;
      return JSON.stringify(copy);
    },
    remove: async (id: string) => {
      const at = files.findIndex((one) => one.id === id);
      if (at !== -1) files.splice(at, 1);
    },
  };
}

describe('the extra-options row writes into the agent file and comes back from it', () => {
  it('stands in More settings, named for the app this agent runs with', () => {
    const claude = plain(markupOf(FORGE));
    expect(
      claude,
      'More settings has to name this row for the app that will receive the lines. Two apps ' +
        'take extra settings in two different shapes, so a row named for neither of them ' +
        'teaches the wrong shape to whoever fills it in',
    ).toContain('Extra options for Claude Code');
    expect(
      claude,
      'and it may name only the app this agent runs with. Both names at once turns one row ' +
        'into two questions and the person answers the wrong one',
    ).not.toContain('Extra options for Codex');

    const codex = plain(markupOf(CODIE));
    expect(codex, 'the same row, named for the other app').toContain('Extra options for Codex');
    expect(codex, 'and again only the app this agent runs with').not.toContain(
      'Extra options for Claude Code',
    );

    expect(
      findField(MoreSettings({ value: FORGE, onChange: noop }), FIELD),
      'the words are not the setting: there has to be a real control carrying data-field="' +
        FIELD +
        '", the same marker the four rows beside it carry',
    ).not.toBeNull();
  });

  it('keeps what you type under the app that runs this agent, and shows it again', () => {
    const claude = typed(FORGE, CLAUDE_TEXT);
    expect(
      claude.vendorOptions,
      'one pair per line, and the pairs land under the key the agent file uses for this app. ' +
        'The name here is claude, not claude-code: the reader on the other side looks the ' +
        'lines up by that word, and a map filed under any other one reaches nobody',
    ).toEqual({ claude: CLAUDE_KEPT });
    expect(
      linesOf(shown(claude)),
      'and reopening the agent shows the same lines, in the order they were written. A round ' +
        'trip that reorders or drops a line makes the person type it again next time',
    ).toEqual(CLAUDE_LINES);

    const codex = typed(CODIE, CODEX_TEXT);
    expect(
      codex.vendorOptions,
      'the other app writes key=value instead of --flag value, and lands under its own key. ' +
        'One shape for both apps passes every question asked about one of them and breaks the ' +
        'other on its first real run',
    ).toEqual({ codex: CODEX_KEPT });
    expect(linesOf(shown(codex)), 'and the same lines come back for it too').toEqual(CODEX_LINES);
  });

  it('carries the lines to the disk and shows them again when the agent is reopened', async () => {
    const disk: Agent[] = [];
    const store = createAgentsStore(diskThatKeeps(disk));
    const went = await store.getState().save(typed(FORGE, CLAUDE_TEXT));

    expect(
      went,
      'the store is the only edge to the disk, and an agent whose only change is these lines has ' +
        'to go through it. Every other question in this file is about a value passed from one ' +
        'function to the next, and a value that never leaves the screen is not a saved setting',
    ).toBe(true);
    expect(
      disk[0]?.vendorOptions,
      'the lines have to reach the file itself. The reader on the other side opens that file and ' +
        'nothing else: a passthrough that lives in the screen and not on disk reaches nobody',
    ).toEqual({ claude: CLAUDE_KEPT });

    /* PONOWNE OTWARCIE, i to jest ta połowa zdania z TASK.md, której nie było: nowy magazyn,
     * ten sam plik. Wszystko powyżej przechodzi też dla formularza, który trzyma wpisany tekst
     * we własnym stanie i gubi go przy pierwszym przeładowaniu okna. */
    const reopened = createAgentsStore(diskThatKeeps(disk));
    await reopened.getState().load();
    const back = reopened.getState().agents.find((one) => one.id === FORGE.id);
    expect(
      back,
      'the agent has to come back from the file at all, or there is nothing to reopen',
    ).toBeDefined();
    if (back === undefined) return;

    expect(
      linesOf(shown(back)),
      'and reopening it puts the same lines, in the same order, back into the box the person ' +
        'typed them into. An empty box over a file that has them is how a person learns to ' +
        'write a setting twice',
    ).toEqual(CLAUDE_LINES);

    /* I NA EKRANIE, nie tylko we właściwości kontrolki (niezmiennik 29). Wartość zwrócona przez
     * komponent dowodzi, że mechanizm jest; markup dowodzi, że człowiek to widzi. */
    const html = renderToStaticMarkup(
      <AgentForm value={back} expanded onChange={noop} onToggleMore={noop} onSave={noop} />,
    );
    for (const line of CLAUDE_LINES) {
      expect(
        plain(html),
        'and the line stands in the open form, where the person looks for it: "' + line + '"',
      ).toContain(line);
    }
  });

  it('will not save a line that has no value, and says which line', async () => {
    const half = typed(FORGE, HALF_TEXT);
    const sentence = missingForSave(half);

    expect(
      sentence,
      'a line with a name and no value has to stop the save. A flag handed over without its ' +
        'value swallows the next argument as its own, so the command means something other ' +
        'than it looks like — and nothing on screen would say so',
    ).not.toBeNull();
    expect(
      sentence ?? '',
      'and the sentence has to name the line to delete. A refusal without it teaches the ' +
        'person that extra options do not work, so they write the same thing again another way',
    ).toContain(HALF_LINE);

    const html = renderToStaticMarkup(
      <AgentForm value={half} expanded onChange={noop} onToggleMore={noop} onSave={noop} />,
    );
    expect(
      plain(html),
      'the sentence belongs on screen, under the button it disarms. A refusal only the store ' +
        'knows about is a Save that does nothing twice in a row',
    ).toContain(sentence ?? '');
    expect(
      /\bdisabled\b/.test(buttonAttributes(html, 'Save') ?? ''),
      'and Save has to be genuinely unusable meanwhile. A button that looks off and still ' +
        'writes the file is the worse half of the lie',
    ).toBe(true);

    const saved: Agent[] = [];
    const store = createAgentsStore(ioThatRecords(saved));
    const went = await store.getState().save(half);

    expect(
      went,
      'and the store, which is the only edge to the disk, has to turn the same agent away',
    ).toBe(false);
    expect(
      saved,
      'nothing may reach the disk on the way. An agent written with half a pair passes the ' +
        'library reader and falls over later, in the middle of a run somebody was waiting on',
    ).toEqual([]);
    expect(
      store.getState().refusal,
      'and it has to be the same sentence, word for word. Two wordings of one rule are two ' +
        'answers to one question (invariant 13)',
    ).toBe(sentence);
  });

  it('hides the other app lines instead of deleting them', () => {
    const both: Agent = { ...FORGE, vendorOptions: { claude: CLAUDE_KEPT, codex: CODEX_KEPT } };

    expect(
      linesOf(shown(both)),
      'the row belongs to the app named in Runs with, so only its lines are on screen',
    ).toEqual(CLAUDE_LINES);
    expect(
      linesOf(shown({ ...both, runsWith: 'codex' })),
      'and switching the app in Runs with shows the other set, not an empty box',
    ).toEqual(CODEX_LINES);

    const edited = typed({ ...both, runsWith: 'codex' }, 'model_verbosity=low');
    expect(
      edited.vendorOptions?.codex,
      'editing the visible lines rewrites the visible ones',
    ).toEqual({ model_verbosity: 'low' });
    expect(
      edited.vendorOptions?.claude,
      'and leaves the hidden ones exactly as the file had them. Switching which app an agent ' +
        'runs with is a thing people do to compare the two, and losing the settings of the one ' +
        'they switched away from is silent — they find out on the next run of the other app',
    ).toEqual(CLAUDE_KEPT);
  });
});
