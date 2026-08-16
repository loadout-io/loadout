/* Kryterium 2 dla T-26: sekcja Agents montuje się naprawdę i przy zerze dalej zaprasza.
 *
 * Powód dwóch połów, kontroli przeciw pustej asercji i dwóch sposobów zapisania zdania jest
 * wypisany raz, w `src/sections/workflows/mounted.test.tsx`. Tutaj różnica jest jedna i to ona
 * jest treścią tego kryterium: PUSTY STAN. Sprawdzenie wyłącznie przypadku z dwoma agentami
 * przechodzi na ekranie, który przy zerze renderuje pustkę bez wyjścia — czyli w jedynym
 * stanie, jaki użytkownik zobaczy przy pierwszym uruchomieniu, bo warstwy IPC jeszcze nie ma
 * (T-07). Dlatego oba przypadki i RÓWNA liczba kontrolek dodawania w obu.
 *
 * `No agents yet.` jest wpisane tutaj ręcznie i jest kontraktem: sekcja ma mieć WŁASNE zdanie
 * pustego ekranu, a nie przepisywać zdanie z rejestru. Zdanie rejestru jest czytane przez
 * `sectionEntry('agents').empty` — jedno miejsce, w którym ono mieszka (niezmiennik 13).
 *
 * Vendor jest sprawdzany przez DWA różne: jeden agent biegnie na Claude Code, drugi na Codex.
 * Ekran, który wypisuje jedną etykietę przy wszystkich, przewraca się na drugiej — a lista
 * samych nazw nie mówi tego, co człowiek przyszedł tu przeczytać. Nazwy z drutu
 * (`claude-code`, `codex`) na ekranie nie istnieją (niezmiennik 14), więc porównujemy
 * z brzmieniami z `agent-form.tsx`.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import type { Agent, AgentsIo, Vendor } from '../../state/agents';
import { createAgentsStore } from '../../state/agents';
import { sectionEntry } from '../../ui/sections';
import AgentsScreen from './index';

/** Zdanie pustego ekranu AGENTÓW — nie zdanie pustej sekcji z rejestru. */
const NO_AGENTS_YET = 'No agents yet.';

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

function agent(id: string, name: string, runsWith: Vendor, model: string): Agent {
  return {
    schema: 1,
    id,
    name,
    summary: 'Turns a goal into steps.',
    color: 'clay',
    instructions: 'Write the smallest change that makes the checks pass.',
    runsWith,
    model,
    thinking: 'balanced',
    fileAccess: 'look-only',
    giveUpAfterMinutes: 10,
    tools: 'everything',
    skills: [],
    connections: [],
    writeResultsTo: '',
  };
}

/** Atrapa dysku: `list` oddaje to, co zasialiśmy, a reszta nie ma prawa zostać zawołana. */
function ioWith(agents: readonly Agent[]): AgentsIo {
  return {
    list: () => Promise.resolve([...agents]),
    newId: () => {
      throw new Error('the seeded library never asks for a fresh id');
    },
    save: () => {
      throw new Error('the seeded library never writes to disk');
    },
    remove: () => {
      throw new Error('the seeded library never removes anything');
    },
  };
}

describe('the agents section mounts for real and keeps inviting when it is empty', () => {
  it('mounts through real discovery and invites instead of reporting a lack of data', () => {
    const markup = renderToStaticMarkup(<App section="agents" />);

    expect(
      markup,
      'asking the shell for agents WITHOUT handing it screens has to reach the file on disk. ' +
        'The agent form has been landed and green since T-11 and was mounted by nobody',
    ).toContain(NO_AGENTS_YET);
    expect(
      occurrences(markup, 'data-create'),
      'an empty screen is an invitation, not a notice that there is nothing (DESIGN §6), so ' +
        'exactly one way to add an agent is on screen at zero. This is the only state a person ' +
        'sees on a first run',
    ).toBe(1);
    expect(
      markup,
      'the section has its own empty sentence now, so the one the registry keeps for agents ' +
        'has no business being in the document as well (invariant 13)',
    ).not.toContain(sectionEntry('agents').empty);
  });

  it('control: with no screen in hand the shell still says the registry sentence', () => {
    const markup = renderToStaticMarkup(<App section="agents" screens={{}} />);

    expect(
      markup,
      'the control against an empty assertion: without it, "the registry sentence is gone" ' +
        'also passes on a shell that stopped rendering that sentence at all',
    ).toContain(sectionEntry('agents').empty);
  });

  it('shows both agents with the vendor each one runs on, and the same one way to add', async () => {
    const store = createAgentsStore(
      ioWith([
        agent('a-1', 'Orion', 'claude-code', 'opus'),
        agent('a-2', 'Needle', 'codex', 'gpt-5.6-sol'),
      ]),
    );
    await store.getState().load();

    const markup = renderToStaticMarkup(<AgentsScreen store={store} />);

    expect(markup, 'the first agent the store holds has to reach the document').toContain('Orion');
    expect(markup, 'and the second one too, not just the first').toContain('Needle');
    expect(
      markup,
      'each agent carries the vendor it runs on. The two seeded agents run on different ones ' +
        'on purpose: a screen that prints one label for everybody passes a list of names and ' +
        'falls over here',
    ).toContain('Claude Code');
    expect(markup, 'and the other agent names the other vendor').toContain('Codex');
    expect(
      occurrences(markup, 'data-create'),
      'the same single way to add an agent as at zero — not one more because the list is no ' +
        'longer empty. Two ways to make a file are two places for it to be made differently',
    ).toBe(1);
  });
});
