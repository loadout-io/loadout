/* „No agents yet." nie ma prawa paść, zanim ktokolwiek zajrzał na dysk.
 *
 * WADA, KTÓRĄ TO KRYTERIUM SĄDZI, zgłoszona przez właściciela 2026-08-31 i identyczna z tą
 * w sekcji Skills. Magazyn wstaje z pustą listą, odczyt katalogu biegnie dopiero w efekcie po
 * zamontowaniu — więc PIERWSZE zdanie, które człowiek z osiemnastoma agentami na dysku czyta
 * o swojej maszynie, brzmi „nie masz żadnego". „Nikt jeszcze nie patrzył" i „nic tam nie ma"
 * to dwa różne zdania i tylko jedno z nich jest w tej chwili prawdziwe.
 *
 * TRZY STANY, NIE DWA — i trzeci nie jest ozdobą. Katalog, którego NIE DA SIĘ przeczytać,
 * czyta się na ekranie identycznie jak katalog pusty; dokładnie ta pomyłka trzymała sekcję
 * pustą przez kilkanaście godzin (nagłówek `./index.tsx`). Zaproszenie „＋ Create" postawione
 * pod zdaniem „nie udało się przeczytać" jest przy tym zachętą do pisania w ciemno.
 *
 * SŁABĄ WERSJĄ jest asercja na samym magazynie — `expect(store.getState().library).toBe(
 * 'reading')`. Przechodzi nad ekranem, który tego pola nigdy nie czyta, czyli nad dokładnie tą
 * klasą wady, dla której powstał niezmiennik 29: kryterium zielone, funkcja martwa. Dlatego
 * wszystkie trzy stany są tu czytane Z MARKUPU, tak jak czyta je człowiek.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Agent, AgentsIo } from '../../state/agents';
import { createAgentsStore } from '../../state/agents';
import AgentsScreen from './index';

/** Zdanie pustej biblioteki. Wpisane ręcznie: to jest kontrakt tego kryterium. */
const NO_AGENTS_YET = 'No agents yet.';

/** Zdanie, którym ekran mówi, że właśnie patrzy. Też kontrakt, nie import. */
const READING = 'Reading the agents you have saved';

const COULD_NOT_READ = 'the agents folder is not readable, so nothing could be listed';

function agent(): Agent {
  return {
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
    writeResultsTo: '',
  };
}

/** Atrapa dysku, która ODPOWIADA tym, co jej podano — albo odmawia zdaniem Rusta. */
function io(answer: readonly Agent[] | string): AgentsIo {
  return {
    list: () =>
      typeof answer === 'string' ? Promise.reject(answer) : Promise.resolve([...answer]),
    newId: () => Promise.resolve('a-new'),
    save: () => Promise.resolve('after-the-save'),
    remove: () => Promise.resolve(),
  };
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

describe('the empty agents screen tells reading, nothing and unreadable apart', () => {
  it('does not say the library is empty before anything has looked at it', () => {
    /* Magazyn dokładnie taki, z jakim wstaje okno: `load()` biegnie w efekcie i jeszcze nie
     * wrócił, a `renderToStaticMarkup` efektów nie uruchamia. */
    const store = createAgentsStore(io([agent()]));

    const markup = renderToStaticMarkup(<AgentsScreen store={store} usage={null} />);

    expect(
      markup,
      'the first thing a person with eighteen agents on disk reads about their own machine is ' +
        'that they have none. The list is empty because nobody has looked yet',
    ).not.toContain(NO_AGENTS_YET);
    expect(
      markup,
      'and the screen has to say what it is doing instead — an empty rectangle with no sentence ' +
        'in it reads as a section that failed',
    ).toContain(READING);
    expect(
      markup,
      'the moving dots say the reading is still going. Without them a person cannot tell a slow ' +
        'read from a screen that stopped (DESIGN §7, the .thinking primitive)',
    ).toContain('data-reading');
    expect(
      occurrences(markup, 'data-empty'),
      'and the invitation is not standing under it: two answers to "what is in this folder", ' +
        'one of them made up (invariant 13)',
    ).toBe(0);
  });

  it('invites once the folder really answered with nothing', async () => {
    /* KONTROLA PRZECIW NADGORLIWEJ POPRAWCE: bez niej „nie mów, że pusto" przechodzi także na
     * ekranie, który nie mówi już nigdy nic i nie ma jak zacząć. */
    const store = createAgentsStore(io([]));
    await store.getState().load();

    const markup = renderToStaticMarkup(<AgentsScreen store={store} usage={null} />);

    expect(
      markup,
      'the folder answered and it holds nothing, so the empty screen is an invitation again ' +
        '(DESIGN §6)',
    ).toContain(NO_AGENTS_YET);
    expect(markup, 'the reading is over, so that sentence has to go').not.toContain(READING);
    expect(
      occurrences(markup, 'data-create'),
      'exactly one way in at zero, the same as before this fix',
    ).toBe(1);
  });

  it('never stands the refusal and the invitation side by side', async () => {
    const store = createAgentsStore(io(COULD_NOT_READ));
    await store.getState().load();

    const markup = renderToStaticMarkup(<AgentsScreen store={store} usage={null} />);

    expect(
      markup,
      'the refusal never reached the screen, so everything below would be a statement about a ' +
        'screen that says nothing at all',
    ).toContain(COULD_NOT_READ);
    expect(
      markup,
      'the screen says it could not read the folder AND that the folder is empty, one under ' +
        'the other. One of those two is false and a person has no way to tell which',
    ).not.toContain(NO_AGENTS_YET);
    expect(
      markup,
      'and it must not say it is still reading either: three states, and the third one has ' +
        'ended',
    ).not.toContain(READING);
    expect(
      occurrences(markup, 'data-create'),
      'and the offer to create goes with it. "＋ Create" under a sentence saying the folder ' +
        'cannot be read is an offer to write into the dark',
    ).toBe(0);
    expect(
      occurrences(markup, COULD_NOT_READ),
      'the sentence stands in ONE place. Said twice — once in a bar and once where the ' +
        'invitation would be — it reads as two different things going wrong (invariant 13)',
    ).toBe(1);
  });

  it('keeps saying it is reading when a second read starts over a full list', async () => {
    const store = createAgentsStore(io([agent()]));
    await store.getState().load();

    const listed = renderToStaticMarkup(<AgentsScreen store={store} usage={null} />);
    expect(listed, 'the agent that is on disk is on screen').toContain('Forge');
    expect(
      listed,
      'and with something to show, the sentence about reading has no business being there',
    ).not.toContain(READING);
  });
});
