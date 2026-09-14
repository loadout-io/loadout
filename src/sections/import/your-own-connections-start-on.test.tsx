/* Twoje własne połączenia startują zaznaczone, połączenia z plików projektu nie.
 *
 * ZGŁOSZENIE WŁAŚCICIELA, 2026-09-11: „domyślnie to powinno wszystko być wypełnione, zwłaszcza
 * connections, a nie że ja mam sam pisać". `linear-server` leży w jego `~/.claude.json` i używa
 * go codziennie; `context7` przyjeżdża z `.mcp.json` sklonowanego repo i jest cudzym poleceniem
 * do uruchomienia. Ptaszek postawiony za niego ma dotyczyć pierwszego, nigdy drugiego.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ImportSetup, type ImportPreview, yoursAmong } from './setup';

const PREVIEW: ImportPreview = {
  snapshot: {
    root: '/project',
    items: [
      {
        id: 'agent',
        kind: 'agent',
        path: '.claude/agents/build.md',
        name: 'build',
        summary: 'Agent',
      },
    ],
  },
  draft: {
    sourceHashes: { '.claude/agents/build.md': 'abc' },
    items: [],
    agents: [{ id: 'a', name: 'Build' }],
    skills: [],
    connections: [
      { id: 'context7', name: 'context7', enabled: false, origin: 'project' },
      { id: 'linear-server', name: 'linear-server', enabled: false, origin: 'yours-here' },
    ],
    workflows: [],
    report: { mappings: [{ itemId: 'agent', compatibility: 'exact', message: 'Ready.' }] },
  },
};

/** Sam znacznik pola wyboru tego połączenia — albo `''`, kiedy takiego pola na ekranie nie ma. */
function tickFor(html: string, connection: string): string {
  return new RegExp(`<input[^>]*data-field="${connection}"[^>]*>`).exec(html)?.[0] ?? '';
}

describe('the connections a person already uses', () => {
  const html = renderToStaticMarkup(
    <ImportSetup initialPreview={PREVIEW} onClose={() => undefined} onImported={() => undefined} />,
  );

  it('comes with your own already ticked and the project’s still off', () => {
    expect(tickFor(html, 'linear-server'), 'this connection has no tick on the screen').not.toBe(
      '',
    );
    expect(tickFor(html, 'context7'), 'this connection has no tick on the screen').not.toBe('');

    expect(
      tickFor(html, 'linear-server'),
      'this one is the person’s own setting, which they use every day — asking for the tick is ' +
        'asking them to repeat a decision they already made',
    ).toContain('checked');
    expect(
      tickFor(html, 'context7'),
      'and this one came out of a cloned repository, where "command": "npx" is somebody else’s ' +
        'code waiting to be run',
    ).not.toContain('checked');
  });

  it('stops telling a person that everything starts off', () => {
    expect(html).not.toContain('Connections stay off unless you enable them');
    expect(html).toContain('Your own connections start on');
  });

  it('says what those ticks will do to the agents, before anything is imported', () => {
    expect(
      html,
      'a person turns five tool servers on and gets agents that can see none of them; the screen ' +
        'has to say what the ticks mean while it can still be changed',
    ).toContain('Agents that name no connection get the ones you turn on here.');
    expect(html).toContain('Agents that already name their own keep them.');
  });

  it('answers the same way for every later Scan', () => {
    expect(
      yoursAmong(PREVIEW),
      'one rule read by the first open and by every Scan after it — two copies drift apart at ' +
        'the first scope somebody adds',
    ).toEqual(['linear-server']);
  });
});
