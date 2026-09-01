/* Kwadrat tożsamości przydziela się sam, zmienia się klikiem w niego, a pytanie przed usunięciem
 * mówi, ILE workflow straci agenta.
 *
 * # Skąd to zadanie
 *
 * `Colour` był wierszem formularza — czwartym z czternastu, i stał NAD `Instructions`, czyli nad
 * jedynym polem, które jest całą treścią agenta. Wymagał zera decyzji: miał działającą wartość
 * domyślną i był dekoracyjny. Wypadł z formularza w całości (`agent-form.tsx`), więc muszą
 * istnieć dwie rzeczy, których wcześniej nie było: token przydzielany bez pytania i miejsce,
 * w którym da się go zmienić.
 *
 * # Dwie połowy, powiedziane na głos
 *
 * „Klikam kwadrat i agent zmienia barwę" jest w tym repo niesprawdzalne w jednym kawałku:
 * `renderToStaticMarkup` nigdy nie odpala `onClick`, a `jsdom` nie ma i nie będzie
 * (`package.json` jest na liście DENIED w `checks/quick-scope.sh`). Pytamy więc osobno i mówimy
 * wprost, że osobno: (1) element, w który człowiek klika, JEST przyciskiem, stoi na kafelku
 * i nie jest zagnieżdżony w przycisku otwierającym agenta — bo przycisk w przycisku nie jest
 * poprawnym dokumentem i w przeglądarce nie działa; (2) funkcja, którą ten handler woła,
 * przechodzi przez wszystkie pięć tokenów i wraca na początek. Sama obecność `<button>` nie jest
 * dowodem, że cokolwiek się stanie, i ten komentarz istnieje po to, żeby nikt jej za dowód
 * nie wziął — dokładnie tak, jak mówi to nagłówek `library-is-reachable.test.tsx`.
 *
 * # Zdanie przed usunięciem
 *
 * Mówiło „Steps that use it will have nothing to run." — ogólnie, choć TEN SAM komponent
 * renderuje liczbę workflow szesnaście wierszy wyżej, w wierszu `used in 3 workflows`. Człowiek
 * czytał więc pytanie, na które nie da się odpowiedzieć: „coś straci" to nie jest informacja,
 * a „nic nie straci" i „trzy workflow przestaną biec" to dwie różne decyzje. Liczba kosztuje
 * jedno wyrażenie i jest jedyną wersją, która pozwala tę decyzję podjąć.
 *
 * Trzy stany, bo trzy niosą treść, i `null` jest trzecim: katalog workflow, którego NIE UDAŁO
 * SIĘ przeczytać, nie ma prawa dać „No workflow uses it." (niezmiennik 17 — to zdanie byłoby
 * nieprawdziwe, a nie ostrożne).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Agent, AgentsIo, Color } from '../../state/agents';
import { createAgentsStore } from '../../state/agents';
import AgentsScreen, { identityFor, nextIdentity } from './index';

function agent(over: Partial<Agent> = {}): Agent {
  return {
    schema: 1,
    id: 'a-1',
    name: 'Forge',
    summary: 'Writes code',
    color: 'clay',
    instructions: 'Write the smallest change that makes the checks pass.',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'work-freely',
    giveUpAfterMinutes: 30,
    tools: 'everything',
    reachesTheWeb: false,
    skills: [],
    connections: [],
    writeResultsTo: '',
    ...over,
  };
}

function ioWith(agents: readonly Agent[]): AgentsIo {
  return {
    list: () => Promise.resolve([...agents]),
    newId: () => Promise.resolve('a-new'),
    save: () => Promise.resolve('after-the-save'),
    remove: () => Promise.resolve(),
  };
}

async function screenOf(
  agents: readonly Agent[],
  over: { usage?: Record<string, number> | null; opened?: Agent; confirming?: boolean } = {},
): Promise<string> {
  const store = createAgentsStore(ioWith(agents));
  await store.getState().load();
  /* Rozłożenie warunkowe, a nie `opened={over.opened}`: `exactOptionalPropertyTypes` odróżnia
     „nie podano" od „podano undefined", i tylko to pierwsze jest tym, o co chodzi. */
  return renderToStaticMarkup(
    <AgentsScreen
      store={store}
      usage={over.usage ?? null}
      {...(over.opened === undefined ? {} : { opened: over.opened })}
      {...(over.confirming === undefined ? {} : { confirming: over.confirming })}
    />,
  );
}

/** Znacznik otwierający element, który niesie ten atrybut. */
function tag(markup: string, attribute: string): string {
  const at = markup.indexOf(attribute);
  if (at < 0) return '';
  const open = markup.lastIndexOf('<', at);
  const close = markup.indexOf('>', at);
  return close < 0 ? '' : markup.slice(open, close + 1);
}

/** Wszystko, co stoi między znacznikiem kafelka a kwadratem tego samego kafelka. */
function between(markup: string, from: string, to: string): string {
  const start = markup.indexOf(from);
  const end = markup.indexOf(to, start);
  return start < 0 || end < 0 ? '' : markup.slice(start, end);
}

describe('the identity square is where the colour is chosen', () => {
  it('is a control on the tile, and not one buried inside another one', async () => {
    const markup = await screenOf([agent()]);
    const square = tag(markup, 'data-identity="clay"');

    expect(square, 'every tile carries an identity square').not.toBe('');
    expect(
      square.startsWith('<button'),
      'the colour row left the form because it needed no decision, so this square is the only ' +
        'place left where the colour can be changed at all. A span here is a decoration that ' +
        'took a setting away',
    ).toBe(true);
    expect(
      /aria-label="[^"]+"/.test(square),
      'and it says what it does out loud: a button whose whole content is one letter tells a ' +
        'screen reader the letter and nothing else',
    ).toBe(true);
    expect(
      / aria-hidden/.test(square),
      'a control hidden from the accessibility tree is a control half the people using this ' +
        'app cannot reach',
    ).toBe(false);

    expect(
      between(markup, 'data-agent="a-1"', 'data-identity="clay"'),
      'the square stands BESIDE the button that opens the agent, not inside it. A button ' +
        'nested in a button is not a document any browser will build the way it is written, ' +
        'and the inner one stops answering clicks',
    ).toContain('</button>');
  });

  it('walks the whole set of identity colours and comes back to the start', () => {
    const first = 'slate' as Color;
    const seen: Color[] = [first];
    let now = first;
    for (let step = 0; step < 4; step += 1) {
      now = nextIdentity(now);
      expect(seen, 'clicking again must not land on a colour already passed').not.toContain(now);
      seen.push(now);
    }

    expect(
      seen.length,
      'the five muted identity values of DESIGN §3 are the whole set, and every one of them ' +
        'has to be reachable by clicking. A cycle that skips one leaves a colour nobody can pick',
    ).toBe(5);
    expect(
      nextIdentity(now),
      'and the sixth click comes back to where it started, so nobody has to know when to stop',
    ).toBe(first);
  });

  it('hands a new agent the next colour instead of always the same one', () => {
    const five = [0, 1, 2, 3, 4].map((taken) => identityFor(taken));

    expect(
      new Set(five).size,
      'a library where every agent is slate is a library where the square says nothing. The ' +
        'square is there to be scannable at a glance, and that is its only job',
    ).toBe(5);
    expect(
      identityFor(5),
      'and the sixth agent starts the set again rather than falling off the end',
    ).toBe(five[0]);
  });
});

describe('deleting an agent says how much is about to lose it', () => {
  it('names the number of workflows, the same number the tile shows', async () => {
    const forge = agent();
    const markup = await screenOf([forge], {
      opened: forge,
      confirming: true,
      usage: { 'a-1': 3 },
    });

    expect(
      markup,
      'the tile already prints this number sixteen rows above, and the question that follows ' +
        'refused to use it. "Something will lose it" is not information: nothing and three ' +
        'workflows are two different decisions',
    ).toContain('Delete Forge? It is used in 3 workflows');
  });

  it('says workflow in the singular when there is one', async () => {
    const forge = agent();
    const markup = await screenOf([forge], {
      opened: forge,
      confirming: true,
      usage: { 'a-1': 1 },
    });

    expect(
      markup,
      '"used in 1 workflows" is the kind of detail after which a person stops believing the ' +
        'rest of the numbers on the screen',
    ).toContain('Delete Forge? It is used in 1 workflow,');
  });

  it('says plainly that nothing uses it when nothing does', async () => {
    const forge = agent();
    const markup = await screenOf([forge], { opened: forge, confirming: true, usage: {} });

    expect(
      markup,
      'zero is the answer that makes this an easy decision, and it is the one answer the old ' +
        'wording could never give',
    ).toContain('Delete Forge? No workflow uses it.');
  });

  it('falls back to the general sentence when the workflow folder was not read', async () => {
    const forge = agent();
    const markup = await screenOf([forge], { opened: forge, confirming: true, usage: null });

    expect(
      markup,
      'a count printed from a read that never finished is a false sentence, not a cautious one ' +
        '(invariant 17). With nothing counted the question says only what is certainly true',
    ).toContain('Delete Forge? Steps that use it will have nothing to run.');
    expect(/used in \d+ workflow/.test(markup), 'and it invents no number on the way').toBe(false);
  });
});
