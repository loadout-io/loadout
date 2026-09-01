/* Agent zapisany BEZ instrukcji nie ma prawa zabić ekranu Agents.
 *
 * WADA, zmierzona 2026-08-31 w prawdziwym chromium podczas biegu e2e. Konsola powtarzała
 * przy każdym wejściu na sekcję:
 *
 *   TypeError: Cannot read properties of undefined (reading 'replace')
 *   [loadout] screen agents could not render TypeError: …
 *
 * Cała sekcja padała, a granica błędu zamieniała ją w pusty prostokąt — czyli ekran, który
 * dla człowieka wygląda na „nic tu nie ma", a naprawdę się wywrócił. Ani jedno kryterium
 * tego nie widziało, bo każda fikstura w tym repo buduje agenta z kompletu pól.
 *
 * TO NIE JEST WADA FIKSTURY, TYLKO ROZJAZD LUSTRA. Rust deklaruje
 * `pub instructions: Option<String>` (`src-tauri/src/library/agents.rs`), a `Agent` po tej
 * stronie obiecuje `instructions: string`. Backend ma więc pełne prawo przysłać agenta bez
 * tego klucza, a front zakłada, że zawsze jest. Pole czyta trzynaście miejsc; pierwsze z brzegu
 * (`roleWords` na kafelku listy) przewracało całą sekcję, a drugie (`missingForSave`) czeka
 * z tym samym `.trim()` na zapisie.
 *
 * DLACZEGO KRYTERIUM SĄDZI GRANICĘ, A NIE TRZYNASTU CZYTELNIKÓW. Obrona u każdego czytelnika
 * z osobna jest tą samą wadą przełożoną na później: czternasty czytelnik jej nie doda i wada
 * wróci tym samym wejściem. Jedno miejsce, w którym ten fakt się prostuje, to `listDefinitions`
 * — jedyna droga, którą zapisani agenci wchodzą do aplikacji (niezmiennik 13).
 *
 * RZUTOWANIE NIŻEJ JEST TREŚCIĄ KRYTERIUM, NIE OBEJŚCIEM TYPU. `Agent` mówi, że pole jest
 * zawsze; gdyby fikstura się z tym zgodziła, opisywałaby kształt, którego ta wada nie ma.
 * Kryterium musi podać dokładnie ten kształt, który naprawdę przychodzi po drucie — inaczej
 * jest zielone i o niczym nie mówi.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../state/agents';
import type { Definition } from '../../state/library';

const answer = vi.hoisted(() => ({ agents: [] as unknown[] }));

/* Granica Tauriego. W vitest nie ma okna, więc `invoke` jest tu jedyną rzeczą, która musi
 * kłamać — wszystko za nią jest prawdziwym kodem produkcyjnym. */
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (name: string) =>
    name === 'list_agents' ? Promise.resolve(answer.agents) : Promise.resolve(undefined),
}));

const { list, listDefinitions } = await import('./io');

/** Komplet pól, JAKI OPISUJE TYP — bez `instructions`, które dokładamy albo nie. */
function whole(): Omit<Agent, 'instructions'> {
  return {
    schema: 1,
    id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f77',
    name: 'Forge',
    summary: 'Writes code',
    color: 'clay',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'work-freely',
    giveUpAfterMinutes: 20,
    tools: 'everything',
    reachesTheWeb: true,
    skills: [],
    connections: [],
    writeResultsTo: '',
  };
}

/** Kształt, który NAPRAWDĘ przychodzi z Rusta, kiedy agent nie ma instrukcji: bez klucza. */
function withoutInstructions(): Definition<Agent> {
  return { kind: 'healthy', value: whole() } as Definition<Agent>;
}

/**
 * Kształt, KTÓRY NAPRAWDĘ PRZYCHODZI Z GRANICY E2E — cztery klucze z piętnastu.
 *
 * Nie jest zmyślony: to jest dosłownie agent, którego wystawiają fikstury w `e2e/tests/`
 * (`{ id, name, summary, skills: [] }`). Rzutowanie jest treścią kryterium, nie obejściem
 * typu: `Agent` obiecuje wszystkie piętnaście, a wada polega dokładnie na tym, że przychodzi
 * mniej — fikstura zgodna z typem opisywałaby kształt, którego ta wada nie ma.
 */
function fourKeys(): Agent {
  return {
    id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f78',
    name: 'Operator',
    summary: 'Builds the requested change',
    skills: [],
  } as unknown as Agent;
}

beforeEach(() => {
  answer.agents = [withoutInstructions()];
});

describe('an agent saved without instructions still reaches the screen', () => {
  it('straightens the bare shape too, which is the one the browser really hands back', async () => {
    /* DRUGI KSZTALT, i to ON padal w przegladarce. `DefinitionListing` dopuszcza obok
     * opakowanej definicji takze GOLA wartosc; tak odpowiada granica e2e i tak odpowiadaja
     * wstrzykiwane atrapy. Pierwsza wersja tej naprawy pytala wylacznie o `kind === 'healthy'`,
     * wiec gola wartosc szla nietknieta — kryteria wyzej byly zielone, a ekran Agents dalej
     * padal pod chromium. Bez tego punktu naprawa jest zielona i nieprawdziwa. */
    answer.agents = [whole()];

    const healthy = await list();

    expect(
      typeof healthy[0]?.instructions,
      'the agent handed over as a bare value kept its hole where a string was promised, so the ' +
        'tile still asks a missing instruction to replace its whitespace and the whole Agents ' +
        'screen goes down behind the error boundary.',
    ).toBe('string');
  });

  it('hands the screen a readable instruction, not a hole where a string was promised', async () => {
    const [one] = await listDefinitions();

    expect(one, 'nothing came back for the one agent on the shelf.').not.toBeUndefined();
    expect(
      one?.kind,
      'the agent came back marked as a problem. It is not a problem: a person is allowed to ' +
        'save a role and write its instruction later.',
    ).toBe('healthy');

    const value = one?.kind === 'healthy' ? one.value : null;
    expect(
      typeof value?.instructions,
      'the instruction arrived as something other than a string, so every one of the thirteen ' +
        'places that reads it is one call away from throwing. That is what killed the whole ' +
        'Agents screen: the tile asked the instruction to replace its whitespace, the value ' +
        'was not there, and the section became an empty rectangle behind the error boundary.',
    ).toBe('string');
  });

  /* 2026-08-31 WIECZOREM — TA SAMA WADA, DRUGI RAZ, INNE POLE.
   *
   * Do tego wieczora ekran Agents wstawał jako ściana kafelków, a formularz roli montował się
   * WYŁĄCZNIE po kliknięciu w kafelek — więc agent z dziurą w `model` docierał tylko do
   * `roleWords`, czyli do jednego czytelnika, tego naprawionego wyżej. Właściciel kazał ścianę
   * usunąć („a i to powinno byc domyslnie, wyjeb ten widok tu"), i od tej zmiany rola stoi
   * w ciele ekranu OD PIERWSZEJ KLATKI: `AgentForm` czyta pierwszego agenta zawsze. Zmierzone
   * w prawdziwym chromium podczas biegu e2e, sześć razy w jednym pliku specyfikacji:
   *
   *   TypeError: Cannot read properties of undefined (reading 'trim')
   *     at AgentForm (src/sections/agents/agent-form.tsx) — `value.model.trim()`
   *   [loadout] screen agents could not render
   *
   * Kryterium pyta o CAŁY kształt, a nie o `model`, i to jest wybór: naprawa pola po polu jest
   * tą samą wadą przełożoną na później — następny czytelnik trafi w `thinking` albo w `tools`,
   * a granica jest jedna (niezmiennik 13). */
  it('straightens EVERY key the boundary left out, not only the one that fell over first', async () => {
    answer.agents = [fourKeys()];

    const [one] = await list();
    expect(one, 'nothing came back for the one agent on the shelf.').not.toBeUndefined();

    for (const key of [
      'schema',
      'color',
      'instructions',
      'runsWith',
      'model',
      'thinking',
      'fileAccess',
      'giveUpAfterMinutes',
      'tools',
      'reachesTheWeb',
      'skills',
      'connections',
      'writeResultsTo',
    ] as const) {
      expect(
        one?.[key],
        'the key "' +
          key +
          '" came back as a hole where a value was promised. The role now stands in the body of ' +
          'the screen before anybody clicks, so the form reads every one of these on the way in ' +
          'and the first hole takes the whole section down behind the error boundary.',
      ).toBeDefined();
    }

    expect(
      one?.name,
      'and what really did arrive is untouched: filling holes must not overwrite the disk with ' +
        'our own defaults. That would be the same defect wearing the coat of its own repair.',
    ).toBe('Operator');
    expect(one?.summary, 'the same for the sentence about the role').toBe(
      'Builds the requested change',
    );
  });

  it('does not invent words the person never wrote', async () => {
    const [one] = await listDefinitions();
    const value = one?.kind === 'healthy' ? one.value : null;

    expect(
      value?.instructions,
      'the missing instruction was filled in with text of our own. An empty instruction is a ' +
        'true statement about this agent; anything else is the screen telling a person they ' +
        'wrote something they did not.',
    ).toBe('');
  });

  it('keeps the agent on the shelf the rest of the app reads', async () => {
    const healthy = await list();

    expect(
      healthy.length,
      'the agent fell out of the list every other screen reads. Dropping it is not a repair — ' +
        'a role with no instruction yet is still a role a person saved, and it has to appear ' +
        'in the lead picker and in the workflow editor like any other.',
    ).toBe(1);
    expect(healthy[0]?.name, 'the agent came back as somebody else.').toBe('Forge');
  });
});
