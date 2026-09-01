/* Ekran importu nazywa PIĘĆ rodzajów rzeczy, i nazywa je po angielsku.
 *
 * ZGŁOSZENIE WŁAŚCICIELA, 2026-08-29. Podsumowanie planu brzmiało
 * „17 skills · 11 agents · 16 memorys · 1 hook · 4 rules". Dwie wady w jednym wierszu:
 * `memorys` to nie jest angielskie słowo, bo zdanie sklejało literę `s` do KLUCZA rodzaju
 * zamiast użyć etykiety, którą ten sam ekran wypisuje w kolumnie „Type"; a `hook` i `rule`
 * nazywały rzeczy, których Loadout nie umie u siebie postawić — po tej stronie nie ma dla
 * nich ani sekcji, ani wykonawcy.
 *
 * Kryterium sądzi ZDANIE NA EKRANIE, nie wartość funkcji (niezmiennik 29): `itemCounts`
 * mogłoby zwracać cokolwiek, gdyby nikt tego nie renderował, a właśnie ten wiersz właściciel
 * przeczytał na zrzucie.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { ImportItem, ImportPreview, ItemKind } from './setup';
import { ImportSetup } from './setup';

/** Ile pozycji każdego rodzaju wchodzi do planu. Liczby są różne, żeby wiersz je rozróżnił. */
const PLAN: readonly (readonly [ItemKind, number])[] = [
  ['agent', 2],
  ['skill', 3],
  ['connection', 1],
  ['workflow', 4],
  ['memory', 5],
];

function item(kind: ItemKind, at: number): ImportItem {
  const name = `${kind}-${String(at)}`;
  return {
    id: name,
    kind,
    sources: [
      { provider: 'claude', path: `.claude/${name}`, hash: `h-${name}`, role: 'definition' },
    ],
    target: kind === 'memory' ? `memory/notes/${name}.md` : `${kind}s/${name}`,
    dependencies: [],
    status: 'ready',
    statusMessage: 'Loadout can bring this over as it is.',
    generatedHash: null,
  };
}

const ITEMS: readonly ImportItem[] = PLAN.flatMap(([kind, count]) =>
  Array.from({ length: count }, (_, at) => item(kind, at)),
);

const PREVIEW: ImportPreview = {
  snapshot: {
    root: '/project',
    items: ITEMS.map((one) => ({
      id: one.id,
      kind: one.kind,
      path: one.sources[0]?.path ?? '',
      name: one.id,
      summary: 'Found in this project.',
    })),
  },
  draft: {
    sourceHashes: Object.fromEntries(ITEMS.map((one) => [one.id, `h-${one.id}`])),
    items: [...ITEMS],
    agents: [],
    skills: [],
    connections: [],
    workflows: [],
    report: { mappings: [] },
  },
};

/** Liczniki nad tabelą, jako pary „ile czego" w kolejności ekranu. */
function tiles(markup: string): string[] {
  const cells = [
    ...markup.matchAll(
      /<b class="block text-heading text-ink">(\d+)<\/b><small class="text-muted">([^<]*)<\/small>/g,
    ),
  ];
  return cells.map((cell) => `${cell[1] ?? ''} ${cell[2] ?? ''}`);
}

/** Każde słowo, jakie kolumna „Type" wypisała, bez powtórzeń i w kolejności ekranu. */
function typesOnScreen(markup: string): string[] {
  const cells = [...markup.matchAll(/<td class="px-3 py-2 text-body text-ink">([^<]*)<\/td>/g)];
  return [...new Set(cells.map((cell) => cell[1] ?? ''))];
}

/** Plan, w którym jedna rzecz przyszła z dwóch aplikacji naraz. */
const MERGED: ImportPreview = {
  snapshot: {
    root: '/project',
    items: [
      {
        id: 'ship',
        kind: 'skill',
        path: '.agents/skills/ship/SKILL.md',
        name: 'ship',
        summary: 'Found in this project.',
      },
    ],
  },
  draft: {
    sourceHashes: { ship: 'h-ship' },
    items: [
      {
        id: 'ship',
        kind: 'skill',
        sources: [
          {
            provider: 'agent_skills',
            path: '.agents/skills/ship/SKILL.md',
            hash: 'h-here',
            role: 'definition',
          },
          {
            provider: 'claude',
            path: '.claude/skills/ship/SKILL.md',
            hash: 'h-there',
            role: 'definition',
          },
        ],
        target: 'skills/ship/SKILL.md',
        dependencies: [],
        status: 'ready',
        statusMessage: 'Loadout can bring this over as it is.',
        generatedHash: null,
      },
    ],
    agents: [],
    skills: [],
    connections: [],
    workflows: [],
    report: { mappings: [] },
  },
};

function screen(): string {
  return renderToStaticMarkup(
    <ImportSetup onClose={() => undefined} onImported={() => undefined} initialPreview={PREVIEW} />,
  );
}

describe('the import screen names what it brings over', () => {
  it('counts every kind in the plan, notes included', () => {
    const markup = screen();
    /* Kontrola przeciw pustej asercji: bez tego wiersza wszystkie zdania niżej byłyby
     * zdaniami o ekranie, który niczego nie policzył. */
    expect(tiles(markup), 'the screen stopped counting the plan at all').not.toEqual([]);

    expect(tiles(markup), 'a kind that gets imported is missing its own count').toEqual([
      '2 Agents',
      '3 Skills',
      '1 Connections',
      '4 Workflows',
      '5 Notes',
    ]);
  });

  it('names the same kinds in the table as above it', () => {
    const shown = typesOnScreen(screen());
    expect(shown, 'the table stopped naming what each row is').not.toEqual([]);
    expect(shown, 'the table and the counts disagree about what these things are called').toEqual([
      'Agent',
      'Skill',
      'Connection',
      'Workflow',
      'Note',
    ]);
  });

  it('does not list the same files a second time under a heading of its own', () => {
    const markup = screen();
    /* Kontrola przeciw pustej asercji: cele SĄ na ekranie, raz, przy swoich wierszach. */
    expect(markup, 'the rows stopped saying where each thing lands').toContain(
      'Target: agents/agent-0',
    );
    expect(
      markup,
      'the screen repeats every target a second time, and that second list starved the table',
    ).not.toContain('Proposed files');
  });

  it('says out loud what it leaves in the project', () => {
    const markup = screen();
    /* Kontrola przeciw pustej asercji: ekran w ogóle mówi, co robi. */
    expect(markup, 'the screen stopped saying what it brings over').toContain(
      'Turn project agents, skills, connections, workflows, and notes into Loadout files.',
    );
    /* Po zawężeniu do pięciu rodzajów hooki, reguły i ustawienia cudzej aplikacji znikają
     * z listy bez słowa. Milczenie po takiej zmianie czyta się jak przeoczenie, a nie jak
     * decyzja — zwłaszcza że agent i tak czyta je z folderu projektu (zmierzone 2026-08-29). */
    expect(markup, 'the screen says nothing about what it deliberately leaves behind').toContain(
      'Hooks, project rules, and app settings stay in the project — agents read them from the ' +
        'project folder.',
    );
  });

  it('shows every copy a merged row came from', () => {
    const markup = renderToStaticMarkup(
      <ImportSetup
        onClose={() => undefined}
        onImported={() => undefined}
        initialPreview={MERGED}
      />,
    );
    expect(markup, 'the merged row forgot the copy it was built from').toContain(
      '.agents/skills/ship/SKILL.md',
    );
    expect(markup, 'the merged row forgot the copy it was built from').toContain(
      '.claude/skills/ship/SKILL.md',
    );
  });
});
