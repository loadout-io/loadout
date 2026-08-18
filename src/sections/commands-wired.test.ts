/* AC-2 dla T-27: każda krawędź sekcji naprawdę woła komendę, po nazwie z jednej listy.
 *
 * `@tauri-apps/api/core` jest podmieniony atrapą — żadnego żywego Tauri, żadnej przeglądarki.
 * Kryterium, które ich wymaga, nie umie być czerwone z właściwego powodu: `Failed to launch`
 * stoi na liście `NOT_A_REAL_RED` w `harness/gate.py`, i to jest dokładnie ten powód, dla
 * którego szew między warstwami przeżył dwadzieścia sześć zielonych zadań bez ani jednego
 * dowodu.
 *
 * DLACZEGO KAŻDA FUNKCJA JEST WYKONYWANA, A NIE OGLĄDANA.
 * Słaba wersja tego kryterium to grep po `invoke(` w źródłach. Przechodzi na krawędzi, która
 * woła `invoke` w martwej gałęzi, i na takiej, która skleja nazwę komendy ze zmiennej — czyli
 * na dwóch defektach, których nikt nigdy nie zobaczy, dopóki nie kliknie. Rozróżnia je
 * wykonanie: każda funkcja jest wołana naprawdę, nazwa komendy jest odczytana z atrapy, a to,
 * co funkcja dostała, ma się znaleźć w tym, co pojechało do Rusta.
 *
 * KONTROLA PRZECIW PUSTEJ ASERCJI. Dziś wszystkie cztery krawędzie rzucają `not implemented`,
 * więc każdy test, który tylko LICZY wystąpienia `invoke` w plikach, przechodzi na nich bez
 * zmiany ani jednej linii. Stąd asercja jawna: żadna z nich nie ma prawa tak odmówić.
 *
 * TABELA MUSI POKRYWAĆ CAŁY EKSPORT. Wiersze niżej nie są listą przykładów — pierwszy test
 * porównuje je z tym, co moduły faktycznie eksportują, więc funkcja dopisana jutro bez wiersza
 * tutaj jest czerwona. Ręczna lista przykładów milczy dokładnie o tej funkcji, o której autor
 * zapomniał.
 */
import { readFileSync } from 'node:fs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import * as agents from './agents/io';
import * as memory from './memory/io';
import * as skills from './skills/io';
import * as workflows from './workflows/io';

import type { Agent } from '../state/agents';
import type { Import } from '../state/skills';
import type { WorkflowFile } from '../state/workflows';

/* Atrapa jest podniesiona razem z `vi.mock`, żeby moduły sekcji dostały JĄ, a nie prawdziwy
 * transport. Rozwiązuje się zawsze i zawsze tą samą wartością: ta droga nie mierzy odpowiedzi
 * Rusta, tylko to, co w jego stronę pojechało. */
const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((..._sent: unknown[]) => Promise.resolve(undefined)),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

/** Ta sama lista, którą po drugiej stronie granicy czyta `ipc_commands_registered.rs`. */
const GOLDEN = new URL('../../src-tauri/commands.golden.txt', import.meta.url);

const known = new Set(
  readFileSync(GOLDEN, 'utf8')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('#')),
);

const AGENT_ID = '0198a1f2-3b4c-7d5e-8f60-112233445566';
const FILE_NAME = 'ship-a-feature.json';

const AGENT: Agent = {
  schema: 1,
  id: AGENT_ID,
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

const WORKFLOW: WorkflowFile = {
  format: 1,
  id: '0198a1f2-3b4c-7d5e-8f60-99887766aabb',
  name: 'Ship a feature',
  steps: [
    { kind: 'checkpoint', id: 'look', name: 'Does the plan look right?', at: { x: 0, y: 0 } },
  ],
  links: [],
};

const SKILL: Import = {
  name: 'pdf',
  summary: 'Pulls tables out of PDF files.',
  reviewed: { body: '---\nname: pdf\n---\n', findings: [], verdict: 'clean' },
  scripts: 1,
  fromTheInternet: true,
};

/** Jedna krawędź: skąd, co, którą komendę woła, z czym ją wołamy. */
interface Wire {
  readonly where: string;
  readonly what: string;
  readonly command: string;
  /** To, co funkcja dostaje. Każda wartość stąd ma się znaleźć w tym, co pojechało dalej. */
  readonly given: readonly unknown[];
  readonly call: () => unknown;
}

const WIRES: readonly Wire[] = [
  { where: 'agents', what: 'list', command: 'list_agents', given: [], call: () => agents.list() },
  { where: 'agents', what: 'newId', command: 'new_id', given: [], call: () => agents.newId() },
  {
    where: 'agents',
    what: 'save',
    command: 'save_agent',
    given: [AGENT],
    call: () => agents.save(AGENT),
  },
  {
    where: 'agents',
    what: 'remove',
    command: 'delete_agent',
    given: [AGENT_ID],
    call: () => agents.remove(AGENT_ID),
  },
  {
    where: 'workflows',
    what: 'list',
    command: 'list_workflows',
    given: [],
    call: () => workflows.list(),
  },
  {
    where: 'workflows',
    what: 'newId',
    command: 'new_id',
    given: [],
    call: () => workflows.newId(),
  },
  {
    where: 'workflows',
    what: 'load',
    command: 'load_workflow',
    given: [FILE_NAME],
    call: () => workflows.load(FILE_NAME),
  },
  {
    where: 'workflows',
    what: 'write',
    command: 'save_workflow',
    given: [FILE_NAME, WORKFLOW],
    call: () => workflows.write(FILE_NAME, WORKFLOW),
  },
  {
    where: 'workflows',
    what: 'remove',
    command: 'delete_workflow',
    given: [FILE_NAME],
    call: () => workflows.remove(FILE_NAME),
  },
  {
    where: 'workflows',
    what: 'check',
    command: 'check_workflow',
    given: [WORKFLOW],
    call: () => workflows.check(WORKFLOW),
  },
  {
    where: 'skills',
    what: 'readLink',
    command: 'review_skill',
    given: ['https://example.invalid/skills/pdf/SKILL.md'],
    call: () => skills.readLink('https://example.invalid/skills/pdf/SKILL.md'),
  },
  {
    where: 'skills',
    what: 'install',
    command: 'install_skill',
    given: [SKILL],
    call: () => skills.install(SKILL),
  },
  {
    where: 'memory',
    what: 'putToUse',
    command: 'put_note_to_use',
    given: [{ id: 'note-to-use' }],
    call: () => memory.putToUse({ id: 'note-to-use' }),
  },
  {
    where: 'memory',
    what: 'stopUsing',
    command: 'stop_using_note',
    given: [{ id: 'note-to-drop' }],
    call: () => memory.stopUsing({ id: 'note-to-drop' }),
  },
  /* 2026-08-18 (T-38 AC-5/AC-6) — DWIE NOWE KRAWĘDZIE ODCZYTU, DOPISANE, NIC NIE USUNIĘTE.
   * Ten plik złapał je, zanim zdążyły wjechać niezauważone, i to jest dokładnie jego zadanie:
   * `listNotes` i `listSkills` powstały po to, żeby Pamięć i Umiejętności czytały z dysku —
   * bez tych dwóch wierszy byłyby funkcjami, których nikt nigdy nie zobaczył docierających
   * do Rusta, a zapisana umiejętność ginęła po restarcie (niezmiennik 4). */
  {
    where: 'memory',
    what: 'listNotes',
    command: 'list_notes',
    given: [],
    call: () => memory.listNotes(),
  },
  {
    where: 'skills',
    what: 'listSkills',
    command: 'list_skills',
    given: [],
    call: () => skills.listSkills(),
  },
];

const EDGES: ReadonlyArray<readonly [string, object]> = [
  ['agents', agents],
  ['memory', memory],
  ['skills', skills],
  ['workflows', workflows],
];

function exportedFunctions(edge: object): string[] {
  return Object.entries(edge)
    .filter(([, value]) => typeof value === 'function')
    .map(([name]) => name)
    .sort();
}

/**
 * Każda wartość prosta w środku, na dowolnym poziomie zagnieżdżenia.
 *
 * Porównujemy wartości, a nie klucze: krawędź wolno napisać jako `{ id }` albo `{ agentId }`,
 * bo Tauri i tak przepisuje nazwy argumentów przy przejściu. Czego nie wolno, to zgubić samą
 * wartość — i tylko o to pyta ta funkcja.
 */
function insides(value: unknown, into: unknown[]): unknown[] {
  if (Array.isArray(value)) {
    for (const item of value as unknown[]) insides(item, into);
  } else if (typeof value === 'object' && value !== null) {
    for (const item of Object.values(value as Record<string, unknown>)) insides(item, into);
  } else if (value !== undefined && value !== null) {
    into.push(value);
  }
  return into;
}

describe('the four section edges and the one list of command names', () => {
  beforeEach(() => {
    invoked.mockClear();
  });

  it('has a row below for every function the four edges export', () => {
    for (const [where, edge] of EDGES) {
      const exported = exportedFunctions(edge);
      const covered = WIRES.filter((wire) => wire.where === where)
        .map((wire) => wire.what)
        .sort();
      expect(
        covered,
        'src/sections/' +
          where +
          '/io.ts exports something this file never runs, or this file names something it does ' +
          'not export. A function with no row here is a function nobody ever watched reach Rust.',
      ).toEqual(exported);
    }
  });

  it('names only commands that are on commands.golden.txt', () => {
    const strangers = [...new Set(WIRES.map((wire) => wire.command))].filter(
      (command) => !known.has(command),
    );
    expect(
      strangers,
      'these command names are expected below and are not on src-tauri/commands.golden.txt: ' +
        strangers.join(', ') +
        '. The list is the one place where both sides of the seam agree on a name.',
    ).toEqual([]);
  });

  for (const wire of WIRES) {
    it(
      wire.where + '/' + wire.what + ' calls ' + wire.command + ' once, carrying what it got',
      async () => {
        let refusal: unknown = null;
        try {
          await wire.call();
        } catch (error) {
          refusal = error;
        }

        const said = refusal instanceof Error ? refusal.message : String(refusal ?? '');
        expect(
          said.includes('not implemented'),
          'src/sections/' +
            wire.where +
            '/io.ts turned down ' +
            wire.what +
            ' with "not implemented". That is what all four edges do today, and it is why this ' +
            'test runs them instead of reading them: ' +
            said,
        ).toBe(false);

        expect(
          invoked.mock.calls.length,
          wire.where +
            '/' +
            wire.what +
            ' has to reach Rust exactly once. Zero is the state this task exists to end; more ' +
            'than one means the same button does the same work twice.',
        ).toBe(1);

        const sent = invoked.mock.calls.at(0);
        if (sent === undefined) {
          throw new Error(wire.where + '/' + wire.what + ' never reached Rust at all');
        }

        const name = sent.at(0);
        expect(
          typeof name === 'string' && known.has(name),
          wire.where +
            '/' +
            wire.what +
            ' asked Rust for ' +
            String(name) +
            ', which is not on src-tauri/commands.golden.txt — so nothing on the Rust side is ' +
            'keeping that name alive, and the day it is renamed this call goes quiet.',
        ).toBe(true);
        expect(name, 'and the command it asks for is the one this section is meant to use').toBe(
          wire.command,
        );

        const carried = insides(sent.at(1), []);
        const lost = insides(wire.given, []).filter((value) => !carried.includes(value));
        expect(
          lost,
          wire.where +
            '/' +
            wire.what +
            ' called the right command and left some of what it was given behind: ' +
            JSON.stringify(lost) +
            '. A call that reaches Rust without its values is the same silence as no call at all.',
        ).toEqual([]);
      },
    );
  }
});
