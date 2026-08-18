/* Zapis agenta: co idzie na dysk, co się dzieje przy odmowie i czego dysk nie ma prawa zobaczyć.
 *
 * TO SĄ TRZY RZECZY, KTÓRYCH DO 2026-08-18 NIE SPRAWDZAŁO NIC — a przyczyną tego, że aplikacja
 * była nieosiągalna, była właśnie ich suma. Na maszynie właściciela `~/.loadout/agents` NIE
 * ISTNIAŁ: ani jeden zapisany agent, więc każdy bieg kończył się odmową, bo krok workflow nazywa
 * agenta, a nie było czego nazwać. Zapis jechał `void (async () => { … })()` bez `catch`, obok
 * magazynu, a `AgentsState` nie miało pola na zdanie dla człowieka. Klikasz Save, nic się nie
 * dzieje, drugie kliknięcie identycznie.
 *
 * SŁABA WERSJA TYCH TRZECH ASERCJI I CO JĄ ODRÓŻNIA:
 *
 *   (a) „adapter dostał kompletnego agenta" — słabo: `expect(io.saved).toHaveLength(1)`.
 *       Przechodzi dla `io.save({ name })`, czyli dla pliku bez instrukcji, bez modelu i bez
 *       limitu czasu — a taki plik biblioteka Rusta przyjmie i odrzuci dopiero krok w biegu.
 *       Rozróżnia to porównanie CAŁEGO obiektu (`toEqual`) plus asercja na ZBIÓR KLUCZY: pole
 *       dopisane do typu i zgubione po drodze przez `save` przewraca drugą, choć pierwsza
 *       porównuje tylko to, co test wpisał.
 *
 *   (b) „odmowa wylądowała w stanie" — słabo: `expect(store.getState().refusal).not.toBeNull()`.
 *       Przechodzi dla zdania zapasowego wpisanego na sztywno, czyli dla dokładnie tej awarii,
 *       którą naprawia `src/ipc/why.ts`: Tauri odrzuca NAPISEM, `error instanceof Error` jest
 *       zawsze fałszywe, więc precyzyjna odmowa Rusta ginęła i człowiek czytał zdanie ogólne
 *       przy każdej możliwej przyczynie. Dlatego niżej stoi RÓWNOŚĆ ze zdaniem, które napisał
 *       dysk, plus asercja negatywna na nasze zdanie zapasowe.
 *
 *   (c) „pusty `instructions` nie woła adaptera" — słabo: sprawdzenie atrybutu `disabled` na
 *       przycisku Save. Wygaszony przycisk nie jest obroną przed zapisem: w repo nie ma jsdom,
 *       więc żaden test nie umie kliknąć, a formularz z polem tekstowym wysyła się też Enterem.
 *       Pytamy więc krawędź, nie kontrolkę: `save` ma ODMÓWIĆ, nie tknąć ani `newId`, ani `save`,
 *       i postawić zdanie, które nazywa brakujące pole.
 *
 * Wszystko idzie przez wstrzyknięte `AgentsIo` — magazyn nie zna nazw komend i nie ma tu czego
 * mockować. Że produkcyjna krawędź naprawdę dowozi to do Rusta pod właściwą nazwą, sprawdza
 * osobno `src/sections/agents/save-reaches-disk.test.ts`.
 */
import { describe, expect, it } from 'vitest';
import type { Agent, AgentsIo } from './agents';
import { createAgentsStore } from './agents';

/** Kompletny agent bez identyfikatora — dokładnie to, co panel oddaje przy `＋ Create`. */
function draft(): Agent {
  return {
    schema: 1,
    id: '',
    name: 'Forge',
    summary: 'Writes code',
    color: 'clay',
    instructions: 'Write the smallest change that makes the checks pass.',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'work-freely',
    giveUpAfterMinutes: 20,
    tools: { only: ['Read', 'Edit'] },
    skills: ['rust-tauri'],
    connections: ['github'],
    writeResultsTo: 'handoffs/build.md',
  };
}

const MINTED = '019897b4-8f3a-7c21-9d44-0b6a1e2c5f80';

interface Recorder extends AgentsIo {
  saved: Agent[];
  /** Ile razy mennica została zawołana. Zero jest tu asercją, nie brakiem danych. */
  minted: () => number;
}

/** Atrapa, która ZAPISUJE. `list` oddaje to, co już przyjęła — biblioteka, nie stała. */
function recorder(): Recorder {
  const saved: Agent[] = [];
  let minted = 0;

  return {
    saved,
    minted: () => minted,
    list: () => Promise.resolve([...saved]),
    newId: () => {
      minted += 1;
      return Promise.resolve(MINTED);
    },
    save: (agent: Agent) => {
      saved.push(agent);
      return Promise.resolve();
    },
    remove: () => Promise.resolve(),
  };
}

/** Atrapa, która odmawia dokładnie tak, jak odmawia Tauri: NAPISEM, nie `Error`. */
function refusing(said: string): AgentsIo {
  return {
    list: () => Promise.resolve([]),
    newId: () => Promise.resolve(MINTED),
    save: () => Promise.reject(said),
    remove: () => Promise.resolve(),
  };
}

describe('saving an agent is the one edge to disk, and it says what happened', () => {
  it('hands the disk edge a COMPLETE agent, with the id the mint gave it', async () => {
    const io = recorder();
    const store = createAgentsStore(io);

    const saved = await store.getState().save(draft());

    expect(saved, 'a complete agent over a disk that accepts it comes back as saved').toBe(true);
    expect(io.minted(), 'a new agent has no id yet, so exactly one is minted for it').toBe(1);
    expect(
      io.saved,
      'exactly one file is written. Two calls here is the "second click does the same thing" ' +
        'defect writing two agents instead of none',
    ).toHaveLength(1);

    const written = io.saved[0];
    expect(
      written,
      'every field the panel filled in has to reach the disk untouched, and the id has to be ' +
        'the minted one. A save that drops instructions writes a file the library accepts and ' +
        'the run rejects, hours later',
    ).toEqual({ ...draft(), id: MINTED });

    /* Zbiór kluczy osobno od `toEqual`. `toEqual` porównuje z obiektem, który napisał ten test,
     * więc pole dopisane do `Agent` i zgubione w `save` przechodziłoby przez nie bez śladu —
     * bo test o tym polu nic nie wie. Ta asercja czyta klucze z TYPU, przez świeży szkic. */
    expect(
      Object.keys(written ?? {}).sort(),
      'the written agent carries exactly the keys a draft carries, no fewer. This is the half ' +
        'that fails when a field is added to the schema and forgotten in the save path',
    ).toEqual(Object.keys(draft()).sort());
  });

  it('keeps the id of an agent that already has one, and mints nothing', async () => {
    const io = recorder();
    const store = createAgentsStore(io);
    const known: Agent = { ...draft(), id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f77' };

    await store.getState().save(known);

    expect(
      io.minted(),
      'editing a saved agent must not mint a second id. Steps in saved workflows point at the ' +
        'one it already has, and a new id turns every one of them into a step with nothing to run',
    ).toBe(0);
    expect(io.saved[0]?.id, 'and the file goes out under that same id').toBe(known.id);
  });

  it('puts the sentence the disk wrote into the state, word for word', async () => {
    /* Prawdziwa odmowa biblioteki agentów po stronie Rusta ma tę postać: precyzyjna, o tym
     * KONKRETNYM pliku. Właśnie takie zdania ginęły w siedmiu miejscach frontu. */
    const said = 'the agents folder is read-only, so "Forge" was not written';
    const store = createAgentsStore(refusing(said));

    const saved = await store.getState().save(draft());

    expect(saved, 'a refused save is not a save, and the panel has to learn that').toBe(false);
    expect(
      store.getState().refusal,
      'the sentence the disk wrote reaches the person unchanged. Anything else is the defect ' +
        'src/ipc/why.ts exists for: Tauri rejects with a STRING, so `error instanceof Error` is ' +
        'always false and every precise refusal used to be swallowed',
    ).toBe(said);
    expect(
      store.getState().refusal,
      'and our own fallback sentence has no business being shown when the disk said something ' +
        'better. Without this line the assertion above also passes for a hard-coded sentence',
    ).not.toContain('Loadout could not save');
    expect(
      store.getState().agents,
      'a refused agent is NOT on the list. A row that exists on screen and not on disk is an ' +
        'agent every step can name and no run can find (invariant 4)',
    ).toEqual([]);
  });

  it('refuses an agent with no instructions without touching the disk at all', async () => {
    const io = recorder();
    const store = createAgentsStore(io);

    const saved = await store.getState().save({ ...draft(), instructions: '   ' });

    expect(saved, 'an agent with no instructions is a name, and a name is not saveable').toBe(
      false,
    );
    expect(
      io.saved,
      'the disk edge is never called. This is the assertion a `disabled` attribute cannot make: ' +
        'there is no jsdom here, so no test clicks, and a form with a text field also submits ' +
        'on Enter',
    ).toEqual([]);
    expect(
      io.minted(),
      'and no id is minted either. An id handed out and never written is a hole in a sequence ' +
        'that is supposed to sort by time [T4 §5.1]',
    ).toBe(0);
    expect(
      store.getState().refusal,
      'and the person is told WHICH field, by the label that stands over the control. ' +
        '"Fill in the required fields" would send them looking through nine of them',
    ).toBe('Fill in Instructions to save this agent.');
  });

  it('refuses an agent with no name, and names that field instead of the other one', async () => {
    const io = recorder();
    const store = createAgentsStore(io);

    await store.getState().save({ ...draft(), name: '' });

    expect(io.saved, 'still nothing on disk').toEqual([]);
    expect(
      store.getState().refusal,
      'the sentence names the field that is actually empty. One sentence for both cases would ' +
        'send the person to look at the field they already filled in',
    ).toBe('Fill in Name to save this agent.');
  });

  it('clears the old refusal when the next save works', async () => {
    const io = recorder();
    const store = createAgentsStore(io);

    await store.getState().save({ ...draft(), name: '' });
    expect(store.getState().refusal, 'the refused save left its sentence').not.toBeNull();

    await store.getState().save(draft());

    expect(
      store.getState().refusal,
      'a refusal that outlives the thing it was about is an answer to a click that succeeded. ' +
        'Every action clears it on the way in',
    ).toBeNull();
    expect(
      store.getState().agents.map((agent) => agent.name),
      'and the agent is on the list now',
    ).toEqual(['Forge']);
  });
});
