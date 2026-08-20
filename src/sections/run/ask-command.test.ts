/* `/ask` w wierszu wejścia: którego agenta, co ma zrobić, i co się mówi, gdy tego agenta nie ma.
 *
 * PO CO TO ISTNIEJE. Zamówienie właściciela 2026-08-20: „odpalać nasze workflows/agents".
 * Workflow ma drogę z tego wiersza (`/run`), agent nie ma żadnej — jednostką pracy jest PLIK,
 * więc jeden agent z jednym zdaniem kosztuje wejście do edytora, założenie workflow, postawienie
 * jednego kafelka i powrót. Za najczęstszą czynność dnia.
 *
 * DLACZEGO TO STOI NA CZYSTYCH FUNKCJACH. To repo nie ma jsdom, więc ani Enter, ani Tab nie
 * odpalają się w kryterium, a `renderToStaticMarkup` nie uruchamia efektów. Rozbiór linii
 * zamknięty w komponencie byłby kodem, którego nic nie sądzi — dokładnie ta rodzina, z której
 * wzięło się siedemnaście kłamiących kontrolek. `readAskLine` bierze napis i oddaje decyzję,
 * więc kryterium woła to, co woła Enter.
 *
 * SŁABA WERSJA CAŁEGO TEGO PLIKU: sprawdzić samo `KNOWN`. Przechodzi dla komendy, która stoi
 * w zachęcie i nie jest rozumiana — czyli dla obietnicy w napisie (niezmiennik 16). Rozstrzyga
 * to rozbiór: para (agent, zadanie) i zdanie zachowane co do znaku.
 */
import { describe, expect, it } from 'vitest';

import type { Agent } from '../../state/agents';
import { agentNames, readAskLine } from './ask-command';
import { PROMPT, suggestions, understand } from './entry/entry';
import type { Named } from './run-command';
import { typable } from './run-command';

/** Definicja agenta z biblioteki — pełna, bo to ona jest jednostką pracy dla `/ask`. */
function saved(id: string, name: string): Agent {
  return {
    schema: 1,
    id,
    name,
    summary: 'Writes things down',
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
}

const FORGE = saved('019897b4-8f3a-7c21-9d44-0b6a1e2c5f77', 'Forge');

/**
 * Agent o nazwie z DWÓCH SŁÓW, i on jest tu najważniejszy.
 *
 * Agent nazywa sam siebie po ludzku, a wiersz wejścia przyjmuje SŁOWA: nazwa, której nie da się
 * wpisać jednym tokenem, jest nazwą, której nie da się użyć. Ta sama pułapka, co przy workflow
 * („Ship a feature" wobec `ship-a-feature`), więc i ta sama funkcja.
 */
const NOTE_TAKER = saved('019897b4-8f3a-7c21-9d44-0b6a1e2c5f78', 'Note taker');

const ALL: readonly Agent[] = [FORGE, NOTE_TAKER];

/** Nazwa workflow, która pasuje do tego samego przedrostka, co `note-taker`. */
const WORKFLOWS: readonly Named[] = [{ name: 'note-list', does: 'Note list — 2 steps' }];

describe('/ask names one agent and carries one sentence to it', () => {
  it('stands in the prompt AND is understood, because those are one list', () => {
    /* `string | null`, nie typ komendy: dopóki `/ask` nie stoi w `KNOWN`, tamten typ nie zna
     * tego napisu i kryterium nie skompilowałoby się — a spec, który się nie kompiluje, nie
     * uruchomił niczego i nie sądzi wyroczni. */
    const understood: string | null = understand('/ask forge write the notes');

    expect(
      understood,
      'a command that stands in the prompt and is not understood is a promise made of letters: ' +
        'the list this line runs and the list it shows have to be one value',
    ).toBe('/ask');
    expect(
      PROMPT,
      'the prompt is built from that same list, so /ask shows up in it without anybody writing ' +
        'the name a second time',
    ).toContain('/ask');
  });

  it('completes agent names after /ask, the way workflow names complete after /run', () => {
    const named = agentNames(ALL);

    expect(
      named.map((one) => one.name),
      'the list under the field is what a person types next, so it carries the name in the form ' +
        'you can type: "Note taker" is two words and this line takes one',
    ).toEqual([typable(FORGE.name), typable(NOTE_TAKER.name)]);

    const after = suggestions('/ask no', WORKFLOWS, named).map((one) => one.name);

    expect(
      after,
      'after /ask the names come from the agents and never from the workflows — a workflow ' +
        'offered where this line takes an agent is a suggestion that refuses on Enter',
    ).toEqual([typable(NOTE_TAKER.name)]);
  });

  it('takes the first word as the agent and keeps the sentence character for character', () => {
    const read = readAskLine(ALL, 'note-taker   write   the   notes');

    /* IDENTYFIKATOR, bo to on jedzie na drut: nazwa da się zmienić, identyfikator nie [T3 §3.1]. */
    expect(
      'agent' in read ? read.agent.id : read.refusal,
      'the agent that was named has to win — taking some default one runs somebody else than ' +
        'the person asked for, and looks like success while doing it',
    ).toBe(NOTE_TAKER.id);
    expect(
      'agent' in read ? read.task : 'refused',
      'the sentence reaches the agent character for character; a reading that collapses runs of ' +
        'spaces rewrites the instruction somebody is about to pay for',
    ).toBe('write   the   notes');
  });

  it('refuses a name with nothing after it instead of starting an agent with no instruction', () => {
    const read = readAskLine(ALL, 'forge');
    const unknown = readAskLine(ALL, 'scribe write the notes');

    expect(
      'agent' in read ? 'it started ' + read.agent.name : 'refused',
      'an agent with no instruction is a turn somebody pays for without having asked anything',
    ).toBe('refused');
    /* DWA RÓŻNE PROBLEMY, DWA RÓŻNE NASTĘPNE RUCHY (DESIGN §8). Jedno zdanie na oba znaczy, że
     * człowiek, który zapomniał zadania, dostaje listę nazw, a człowiek z literówką w nazwie
     * dostaje radę, żeby dopisać zadanie — czyli obaj dostają odpowiedź na cudze pytanie. */
    expect(
      'refusal' in read ? read.refusal : '',
      'not knowing the name and not saying what to do are two different problems, so they ' +
        'cannot end in the same sentence',
    ).not.toBe('refusal' in unknown ? unknown.refusal : '');
  });

  it('refuses a name nobody saved, and says which names exist', () => {
    const read = readAskLine(ALL, 'scribe write the notes');

    const refusal = 'refusal' in read ? read.refusal : 'nothing was refused at all';
    expect(refusal, 'a typo in the name must not silently become part of the sentence').toContain(
      'scribe',
    );
    /* WYMIENIA NAZWY, i to jest cała treść tej odmowy. Nazw, których człowiek nie widzi, nie ma
     * jak zgadnąć — powstają z plików w bibliotece (DESIGN §8). W postaci DO WPISANIA, bo lista,
     * z której nie da się przepisać, jest ozdobą. */
    expect(refusal, 'the refusal has to show a name that can be typed').toContain(
      typable(FORGE.name),
    );
    expect(refusal, 'and the other one as well').toContain(typable(NOTE_TAKER.name));
  });
});
