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
 *
 * GDZIE STOI VENDOR — ZMIENIŁO SIĘ 2026-08-31 WIECZOREM, i to jest zmiana MIEJSCA, nie siły.
 * Do tego wieczora ekran wstawał jako ściana kafelków, a wiersz metadanej na każdym kafelku
 * niósł „Claude Code · opus · Balanced · Work freely · gives up after 20m" — czyli pięć faktów,
 * z których każdy jest POLEM formularza. Ściana zniknęła na zlecenie właściciela („a i to
 * powinno byc domyslnie, wyjeb ten widok tu"), a razem z nią ten wiersz: mówił po raz drugi to,
 * co dziś stoi w kontrolce dwadzieścia pikseli obok (niezmiennik 13).
 *
 * Pytanie tego kryterium NIE ZMALAŁO. Brzmiało „czy KAŻDY agent dostaje swojego vendora, a nie
 * jednej etykiety dla wszystkich", i brzmi tak dalej — tylko zadane jest tam, gdzie ten fakt
 * teraz mieszka: w ARKUSZU otwartej roli. Ekran otwiera się na pierwszej roli, więc pierwszą
 * połowę czyta się bez żadnego szwu; drugą podajemy przez `opened`, bo `renderToStaticMarkup`
 * nigdy nie odpala `onClick`, więc przełączenie roli klikiem jest w tym repo niesprawdzalne
 * (ten sam powód i ten sam szew, co w `library-is-reachable.test.tsx`).
 *
 * SŁABĄ WERSJĄ jest `expect(markup).toContain('Claude Code')` nad całym dokumentem: przechodzi
 * także na ekranie, który przypisał etykiety na odwrót albo obie doczepił do jednego agenta.
 * Dlatego każda połowa pyta ARKUSZ o obie nazwy naraz — o jedną, że jest, o drugą, że jej nie
 * ma — i dlatego arkusz musi przy tym nieść nazwę roli, której dotyczy.
 *
 * KONTRAKT NA MARKUP. Każdy wiersz spisu niesie `data-agent` z identyfikatorem agenta, którego
 * otwiera, a arkusz otwartej roli to wszystko od `<aside` do końca dokumentu — arkusz jest
 * ostatnią powierzchnią tego ekranu, więc to wystarcza i nie wymaga liczenia zagnieżdżeń.
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

/** Arkusz otwartej roli: od otwierającego `<aside` do końca dokumentu. */
function sheetOf(markup: string): string {
  const at = markup.indexOf('<aside');
  return at < 0 ? '' : markup.slice(at);
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
    reachesTheWeb: false,
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
  /* 2026-08-31 — DOWÓD ZAMONTOWANIA ZMIENIŁ ZDANIE, i to jest naprawa, nie osłabienie.
   *
   * Do tego dnia stało tu `toContain('No agents yet.')` na ekranie, którego magazyn NIGDY nie
   * czytał dysku: `renderToStaticMarkup` nie odpala efektów, więc `load()` nie zdążył pobiec.
   * Kryterium przyklepywało więc dokładnie tę wadę, którą właściciel zgłosił: sekcja mówiła
   * „nie masz żadnego agenta" o katalogu, w który nikt nie zajrzał. Dowodem zamontowania
   * prawdziwego ekranu (a nie zdania z rejestru) jest dziś zdanie o CZYTANIU — ta sama siła,
   * bo żadne inne miejsce w aplikacji go nie pisze. Zaproszenie przy prawdziwym zerze sądzi
   * `it` niżej, na magazynie, który naprawdę dostał odpowiedź. */
  it('mounts through real discovery and says it is looking, not that there is nothing', () => {
    const markup = renderToStaticMarkup(<App section="agents" />);

    expect(
      markup,
      'asking the shell for agents WITHOUT handing it screens has to reach the real section. ' +
        'The agent form has been landed and green since T-11 and was mounted by nobody',
    ).toContain('Reading the agents you have saved');
    expect(
      markup,
      'and it must NOT say the library is empty before the disk has answered. That sentence, ' +
        'written over a folder holding eighteen agents, is the first thing a person read about ' +
        'their own machine',
    ).not.toContain(NO_AGENTS_YET);
    expect(
      markup,
      'the section has its own sentences now, so the one the registry keeps for agents has no ' +
        'business being in the document as well (invariant 13)',
    ).not.toContain(sectionEntry('agents').empty);
  });

  it('invites with exactly one way in once the disk has answered with nothing', async () => {
    const store = createAgentsStore(ioWith([]));
    await store.getState().load();

    const markup = renderToStaticMarkup(<AgentsScreen store={store} usage={null} />);

    expect(
      markup,
      'an empty screen is an invitation, not a notice that there is nothing (DESIGN §6)',
    ).toContain(NO_AGENTS_YET);
    expect(
      occurrences(markup, 'data-create'),
      'exactly one way to add an agent is on screen at zero. This is the state a person sees ' +
        'on a first run',
    ).toBe(1);
  });

  it('control: with no screen in hand the shell still says the registry sentence', () => {
    const markup = renderToStaticMarkup(<App section="agents" screens={{}} />);

    expect(
      markup,
      'the control against an empty assertion: without it, "the registry sentence is gone" ' +
        'also passes on a shell that stopped rendering that sentence at all',
    ).toContain(sectionEntry('agents').empty);
  });

  it('shows both agents by name and gives the one it opens its own vendor', async () => {
    const orion = agent('a-1', 'Orion', 'claude-code', 'opus');
    const needle = agent('a-2', 'Needle', 'codex', 'gpt-5.6-sol');
    const store = createAgentsStore(ioWith([orion, needle]));
    await store.getState().load();

    const markup = renderToStaticMarkup(<AgentsScreen store={store} />);

    expect(markup, 'the first agent the store holds has to reach the document').toContain('Orion');
    expect(
      markup,
      'and the second one too, not just the first. Every saved role is in the index by name, ' +
        'whichever one happens to be open',
    ).toContain('Needle');

    /* PIERWSZA POŁOWA: bez żadnego szwu. Ekran otwiera się na pierwszej roli, więc to jest
     * dokładnie ten dokument, który człowiek dostaje po wejściu na sekcję. */
    const first = sheetOf(markup);
    expect(
      first,
      'a role has to be standing in the body before anybody clicks. The screen used to open as ' +
        'a wall of cards and this surface only existed behind a click',
    ).toContain('Orion');
    expect(
      first,
      'and it names the vendor THAT role runs on. The two seeded agents run on different ones ' +
        'on purpose: a screen that prints one label for everybody passes a list of names and ' +
        'falls over here',
    ).toContain('Claude Code');
    expect(
      first,
      'the other vendor has no business in that sheet. A screen that swapped the two labels ' +
        'keeps both of them in the document and passes an assertion made over the whole markup',
    ).not.toContain('Codex');

    /* DRUGA POŁOWA, i to jest ta, która pada na zamianie etykiet miejscami. */
    const second = sheetOf(renderToStaticMarkup(<AgentsScreen store={store} opened={needle} />));
    expect(second, 'picking the other role puts THAT role in the body').toContain('Needle');
    expect(second, 'Needle runs on the other vendor, inside its own sheet').toContain('Codex');
    expect(
      second,
      'and Claude Code has no business there — this is the half that fails on a swap',
    ).not.toContain('Claude Code');

    expect(
      occurrences(markup, 'data-create'),
      'the same single way to add an agent as at zero — not one more because the list is no ' +
        'longer empty. Two ways to make a file are two places for it to be made differently',
    ).toBe(1);
  });
});
