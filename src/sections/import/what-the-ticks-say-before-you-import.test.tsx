/* Co mówią ptaszki, ZANIM człowiek kliknie Import — prawdziwy import właściciela, 2026-09-15.
 *
 * Zaznaczył sześć połączeń, przyszły cztery; z trzynastu agentów przyszedł jeden. Dwa zdania,
 * których wtedy na ekranie nie było, stoją tutaj:
 *
 *   * **z którego pliku przyjechało to połączenie.** `figma` była zadeklarowana wyłącznie
 *     w nagłówku `figma-extractor.md`, więc odznaczenie tamtego wiersza zabierało ją razem
 *     z nim — a nic na ekranie nie wiązało jednego z drugim.
 *   * **czego Loadout z tego agenta nie przenosi.** `memory:` i `maxTurns:` były do 2026-09-16
 *     pytaniem, na które są dwie odpowiedzi dające plik identyczny co do bajtu; okno odznaczało
 *     za człowieka każdy wiersz, który nie jest gotowy.
 *
 * CZEGO TEN PLIK NIE DOWODZI, i mówi to wprost: drugi `describe` jest STRAŻNIKIEM renderowania,
 * nie dowodem naprawy. Ptaszek `Import` i zdanie wiersza biorą się w całości ze statusu
 * przysłanego przez Rusta (`typedExcludedIn` odznacza wyłącznie pozycje inne niż `ready`,
 * a `item.statusMessage` renderuje się bezwarunkowo), więc ta specyfikacja przechodziła także
 * przed naprawą. Czerwień tej reguły niesie `import_delivers_what_you_ticked.rs` po stronie
 * Rusta; tutaj stoi po to, żeby regresja w TSX nie zgasiła tego, co tamto wywalczyło.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ImportSetup, type ImportPreview } from './setup';

const AGENT_FILE = '.claude/agents/figma-extractor.md';
const LEAVES_BEHIND =
  'This agent will become a native Loadout agent. Loadout will import this agent, but not its ' +
  'project memory or turn limit.';

const PREVIEW: ImportPreview = {
  snapshot: {
    root: '/urc-monorepo',
    items: [
      {
        id: 'figma-extractor',
        source: 'claude',
        kind: 'agent',
        path: AGENT_FILE,
        name: 'figma-extractor',
        summary: 'Reads a design.',
      },
    ],
  },
  draft: {
    sourceHashes: { [AGENT_FILE]: 'source-agent' },
    items: [
      {
        id: 'figma-extractor',
        kind: 'agent',
        sources: [
          { provider: 'claude', path: AGENT_FILE, hash: 'source-agent', role: 'definition' },
        ],
        target: 'agents/figma-extractor.md',
        dependencies: [],
        status: 'ready',
        statusMessage: LEAVES_BEHIND,
        generatedHash: 'generated-agent',
      },
    ],
    agents: [{ id: 'agent-id', name: 'figma-extractor' }],
    skills: [],
    connections: [
      { id: 'figma', name: 'figma', enabled: false, origin: 'project', source: AGENT_FILE },
      { id: 'linear-server', name: 'linear-server', enabled: false, origin: 'yours-here' },
    ],
    workflows: [],
    report: {
      mappings: [{ itemId: 'figma-extractor', compatibility: 'adjusted', message: LEAVES_BEHIND }],
    },
  },
};

const html = renderToStaticMarkup(
  <ImportSetup initialPreview={PREVIEW} onClose={() => undefined} onImported={() => undefined} />,
);

/** Wiersz ptaszka tego połączenia — od jego pola wyboru do końca etykiety, albo `''`. */
function rowFor(connection: string): string {
  const at = html.indexOf(`data-field="${connection}"`);
  if (at < 0) return '';
  const ends = html.indexOf('</label>', at);
  return ends < 0 ? html.slice(at) : html.slice(at, ends);
}

describe('the tick next to a tool server', () => {
  it('names the file that connection came from', () => {
    expect(rowFor('figma'), 'this connection has no tick on the screen at all').not.toBe('');
    expect(
      rowFor('figma'),
      'this server is declared in one agent header and nowhere else; a person unticking that ' +
        'row has no way to find that out, and the connection used to leave with it',
    ).toContain(AGENT_FILE);
  });

  it('still says who else can see it', () => {
    expect(
      rowFor('figma'),
      'the file answers "which one of my files is this" — it does not answer "is this the team’s ' +
        'setting or mine", and that is the question a person asks first',
    ).toContain('in the project');
    expect(rowFor('linear-server')).toContain('just you, in this project');
  });

  it('says nothing about a file for a connection that has none', () => {
    expect(
      rowFor('linear-server'),
      'your own scopes have no project file to name, and "from" with nothing after it is worse ' +
        'than silence',
    ).not.toContain('from ');
  });
});

describe('the row of an agent that keeps everything Loadout can reproduce', () => {
  it('arrives with Import ticked', () => {
    const tick = /<input[^>]*aria-label="Import this item"[^>]*>/.exec(html)?.[0] ?? '';

    expect(tick, 'the row has no Import tick on the screen').not.toBe('');
    expect(
      tick,
      'a question whose two answers write the same file byte for byte used to untick this row ' +
        'for the person, and twelve of thirteen agents never arrived',
    ).toContain('checked');
  });

  it('says in one sentence what Loadout does not bring over', () => {
    expect(html).toContain(LEAVES_BEHIND);
  });

  it('does not offer a behavior choice there is nothing to choose about', () => {
    expect(html).not.toContain('Without behavior');
  });
});
