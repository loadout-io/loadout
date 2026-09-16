/* Co widzi człowiek, który importuje ten sam projekt DRUGI RAZ — zgłoszenie właściciela,
 * 2026-09-16, Loadout 0.6.2, urc-monorepo.
 *
 * Ekran oddał wtedy jedno czerwone zdanie („agents/project-manager-backlog.md already exists.
 * Nothing was imported.") i ani jednego znaku przy wierszach, które już wylądowały przy pierwszym
 * imporcie. Człowiek miał je znaleźć okiem wśród kilkudziesięciu pozycji i odznaczyć ręcznie —
 * dwa razy z rzędu skończyło się to tym, że produkt nie zadziałał.
 *
 * Trzy rzeczy, których wtedy na ekranie nie było, stoją tutaj: wiersz mówi w kolumnie Status,
 * że to już masz; jego ptaszek `Import` jest pusty zaraz po skanie; a stopka liczy te pozycje
 * OSOBNO od tych, które człowiek odznaczył sam.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ImportSetup, type ImportPreview } from './setup';

const KEEPER = '.claude/agents/project-manager-backlog.md';
const NEWCOMER = '.claude/agents/frontend-dev.md';

/** Wiersz agenta, jak składa go Rust — tylko pola, które ten ekran naprawdę czyta. */
function agentRow(id: string, path: string, alreadyHere: boolean) {
  return {
    id,
    kind: 'agent' as const,
    sources: [
      { provider: 'claude' as const, path, hash: `hash-${id}`, role: 'definition' as const },
    ],
    target: `agents/${id}.md`,
    dependencies: [],
    status: 'ready' as const,
    statusMessage: 'This agent will become a native Loadout agent.',
    generatedHash: `generated-${id}`,
    alreadyHere,
  };
}

const PREVIEW: ImportPreview = {
  snapshot: {
    root: '/urc-monorepo',
    items: [
      {
        id: 'project-manager-backlog',
        source: 'claude',
        kind: 'agent',
        path: KEEPER,
        name: 'project-manager-backlog',
        summary: 'Keeps the backlog.',
      },
      {
        id: 'frontend-dev',
        source: 'claude',
        kind: 'agent',
        path: NEWCOMER,
        name: 'frontend-dev',
        summary: 'Builds the screen.',
      },
    ],
  },
  draft: {
    sourceHashes: { [KEEPER]: 'hash-project-manager-backlog', [NEWCOMER]: 'hash-frontend-dev' },
    items: [
      agentRow('project-manager-backlog', KEEPER, true),
      agentRow('frontend-dev', NEWCOMER, false),
    ],
    agents: [
      { id: 'agent-keeper', name: 'project-manager-backlog' },
      { id: 'agent-newcomer', name: 'frontend-dev' },
    ],
    skills: [],
    connections: [
      { id: 'linear-server', name: 'linear-server', enabled: false, origin: 'yours-here' },
      { id: 'murmur', name: 'murmur', enabled: false, origin: 'yours-everywhere' },
    ],
    alreadyInTheLibrary: ['agents/project-manager-backlog.md', 'connections/linear-server.json'],
    workflows: [],
    report: {
      mappings: [
        {
          itemId: 'project-manager-backlog',
          compatibility: 'exact',
          message: 'This agent will become a native Loadout agent.',
        },
        {
          itemId: 'frontend-dev',
          compatibility: 'exact',
          message: 'This agent will become a native Loadout agent.',
        },
      ],
    },
  },
};

const html = renderToStaticMarkup(
  <ImportSetup initialPreview={PREVIEW} onClose={() => undefined} onImported={() => undefined} />,
);

/** Wiersz tabeli, w którym stoi ta nazwa — albo `''`, kiedy takiego wiersza nie ma. */
function rowOf(name: string): string {
  return html.split('<tr').find((row) => row.includes(`>${name}</b>`)) ?? '';
}

/** Ptaszek „Import" z tego wiersza. */
function importTick(row: string): string {
  return /<input[^>]*aria-label="Import this item"[^>]*>/.exec(row)?.[0] ?? '';
}

/** Wiersz ptaszka tego połączenia — od pola wyboru do końca etykiety. */
function connectionRow(connection: string): string {
  const at = html.indexOf(`data-field="${connection}"`);
  if (at < 0) return '';
  const ends = html.indexOf('</label>', at);
  return ends < 0 ? html.slice(at) : html.slice(at, ends);
}

describe('the row of an item the library already has', () => {
  it('says so in the Status column', () => {
    expect(rowOf('project-manager-backlog'), 'that item has no row at all').not.toBe('');
    expect(
      rowOf('project-manager-backlog'),
      'the screen used to call this row Ready and then refuse the whole import on it, naming a ' +
        'path in a red bar — a person had no way to tell which rows had already landed',
    ).toContain('>Already in your library<');
  });

  it('arrives with Import unticked', () => {
    expect(
      importTick(rowOf('project-manager-backlog')),
      'this row has no Import tick on the screen',
    ).not.toBe('');
    expect(importTick(rowOf('project-manager-backlog'))).not.toContain('checked');
  });

  it('says why it cannot simply come over again', () => {
    expect(
      rowOf('project-manager-backlog'),
      'an empty tick with no sentence next to it reads like a screen that forgot this row',
    ).toContain('Loadout will not replace the file you already have');
  });

  it('leaves every other row ticked', () => {
    expect(importTick(rowOf('frontend-dev'))).toContain('checked');
    expect(rowOf('frontend-dev')).not.toContain('Already in your library');
  });
});

describe('the footer before Import', () => {
  it('counts what stays out because you already have it, apart from what you unticked', () => {
    expect(html).toContain(
      'Ready to import. 1 item(s) will not be imported, including 1 you already have.',
    );
  });
});

describe('the tick of a connection the library already has', () => {
  it('does not start on, even though it is your own', () => {
    expect(connectionRow('linear-server'), 'that connection has no tick at all').not.toBe('');
    expect(
      connectionRow('linear-server'),
      'your own connections start on, but this one is already in the library and Loadout will ' +
        'not write its file again',
    ).not.toContain('checked');
    expect(
      connectionRow('linear-server'),
      'and the row has to say it, or the empty tick reads like Loadout forgot this one',
    ).toContain('already in your library');
  });

  it('still starts on for the ones you do not have yet', () => {
    expect(connectionRow('murmur')).toContain('checked');
  });
});
