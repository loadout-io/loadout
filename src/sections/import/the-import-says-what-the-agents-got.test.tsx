/* Zdanie po zatwierdzeniu powstaje z ODPOWIEDZI KOMENDY, nigdy z zaznaczeń na ekranie.
 *
 * Różnica między „co zaznaczyłem" a „co się stało" jest całą treścią tego produktu
 * (`docs/FOUNDATIONS.md` §2.1). Zdanie zbudowane z ptaszków mówiłoby o zamiarze i nie zauważyłoby
 * agenta pominiętego przez wyścig zapisu — a właśnie takiego agenta `write_agent_file` pomija.
 *
 * DWA OKNA, JEDNO ZDANIE: „Import" w sekcji Agents (`apply_setup`) i „Import setup from project"
 * (`import_project_setup`). Tu sądzimy drogę od odpowiedzi komendy do gotowego zdania; że to
 * zdanie DOCHODZI na ekran po prawdziwym kliknięciu, sądzą `e2e/tests/import-list-stays-visible`
 * i `e2e/tests/project-setup-import-is-selective` — w prawdziwym Chromium, bo tego renderowanie
 * do napisu nie umie (niezmiennik 29, trzeci szczebel).
 */
import { describe, expect, it, vi } from 'vitest';
import { filledAgents, whatTheyGot } from '../../ipc/filled-agents';

const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((_command: string, _sent?: unknown): Promise<unknown> => Promise.resolve({})),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { applySetup } = await import('./io');

const ASKED = { workspace: '/project', expectedSourceHashes: {}, enableConnections: ['figma'] };

describe('what the person reads after Import', () => {
  it('names the agents and the connections the import window really wrote', async () => {
    invoked.mockResolvedValueOnce({
      id: 'receipt-1',
      written: ['agents/lead.md', 'connections/figma.json'],
      enabledConnections: ['figma'],
      filledAgents: [
        { agent: 'lead', connections: ['figma', 'linear-server'] },
        { agent: 'picky', connections: ['figma', 'linear-server'] },
      ],
    });

    const saved = await applySetup(ASKED, null);

    expect(
      whatTheyGot(saved.filledAgents),
      'the names come from what Rust saved, so an agent skipped by a racing edit is not in the ' +
        'sentence either',
    ).toBe('Gave figma and linear-server to lead and picky.');
  });

  it('stays quiet, and standing, when that answer has the wrong shape', async () => {
    invoked.mockResolvedValueOnce({
      id: 'receipt-2',
      written: [],
      enabledConnections: [],
      filledAgents: 'whatever the other side felt like sending',
    });

    const saved = await applySetup(ASKED, null);

    expect(saved.filledAgents, 'invoke<T> is a cast, not a check').toEqual([]);
    expect(
      whatTheyGot(saved.filledAgents),
      'one badly shaped answer may not take a whole screen down with it',
    ).toBeNull();
  });

  it('says the same thing in the project setup window, from its own answer', () => {
    /* Kształt, którym odpowiada `import_project_setup` — ten sam, który czyta modal
     * (`src/ui/project-setup/modal.tsx`). */
    const answered = {
      imported: ['connection:figma', 'connection:linear-server'],
      filledAgents: [
        { agent: 'lead-orchestrator', connections: ['figma', 'linear-server', 'playwright'] },
      ],
    };

    expect(filledAgents(answered.filledAgents)).toHaveLength(1);
    expect(whatTheyGot(answered.filledAgents)).toBe(
      'Gave figma, linear-server and playwright to lead-orchestrator.',
    );
    expect(
      whatTheyGot({ imported: ['agent:writer'] }),
      'an import that switched nothing on says nothing about agents at all',
    ).toBeNull();
  });
});
