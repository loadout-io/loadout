/* Ekran sekcji Agents: nagłówek, jedna ścieżka dodawania i lista agentów — kafelek za kafelkiem
 * jak w makiecie (`docs/mockup/index.html`, `data-screen="agents"`).
 *
 * DLACZEGO PRZYCISK ODSŁANIA FORMULARZ, A NIE TWORZY PLIKU OD RAZU. `＋ Create` na liście
 * workflow tworzy plik natychmiast, bo pusty workflow jest poprawny — pusty AGENT nie jest:
 * `AgentForm` budzi `Save` dopiero, gdy nazwa i instrukcje są wypełnione [T4 §8.1], a agent
 * bez instrukcji to sama nazwa. Kontrolka odsłania więc formularz, czyli robi dokładnie to,
 * co obiecuje (niezmiennik 16), i nie zostawia na dysku pliku, którego walidator odrzuci.
 *
 * CO SIĘ ZMIENIŁO 2026-08-18 I DLACZEGO. Właściciel otworzył aplikację i `~/.loadout/agents`
 * NIE ISTNIAŁ — ani jednego zapisanego agenta, więc każdy bieg kończył się odmową, bo krok
 * workflow nazywa agenta, a nie było czego nazwać. Przyczyna była w tym pliku i miała cztery
 * warstwy, wszystkie po stronie ciszy:
 *
 *   1. Zapis jechał `void (async () => { … })()` BEZ `catch`, obok magazynu, prosto do adaptera.
 *      Odmowa dysku nie miała gdzie wylądować i nie lądowała nigdzie: klikasz Save, nic się nie
 *      dzieje, drugie kliknięcie identycznie. Dziś zapis jedzie przez `store.save`, który ma
 *      pole `refusal`, a to pole jest TUTAJ wyrenderowane. Jedna droga do dysku, nie dwie.
 *   2. Wygaszony `Save` nie mówił, czego brakuje — to naprawia `agent-form.tsx`.
 *   3. Zapisanego agenta NIE DAWAŁO SIĘ otworzyć: kafelek był `<li>` bez handlera, a panel
 *      montował się wyłącznie dla nowego szkicu. Agent zapisany raz zostawał na liście na
 *      zawsze, z każdą literówką w instrukcjach. Dziś kafelek jest `<button>` i otwiera go.
 *   4. `useAgents.delete` i `useAgents.duplicate` NIE MIAŁY produkcyjnego wołającego —
 *      komenda `delete_agent` była zarejestrowana w Rustcie i nieosiągalna z okna. Dziś
 *      oba wołane są z panelu otwartego agenta.
 *
 * POTWIERDZENIE USUNIĘCIA JEST PRAWDZIWYM RENDEREM, nie `window.confirm`. Dialog przeglądarki
 * blokuje webview i zabiera całą sesję pracy — a przy oknie Tauri nie ma go czym odblokować.
 *
 * O migawce serwerowej zustanda i o tym, dlaczego magazyn czyta się tu przez
 * `useSyncExternalStore`, przeczytaj w `src/sections/workflows/index.tsx`.
 */
import type { ReactElement } from 'react';
import { useEffect, useState, useSyncExternalStore } from 'react';
import type { Agent, AgentsIo, Color, FileAccess, Thinking } from '../../state/agents';
import { createAgentsStore } from '../../state/agents';
import { problemSays } from '../../state/library';
import { ImportSetup } from '../import';
import { AgentForm, VENDORS } from './agent-form';
import * as Disk from './io';
import { readUsage, usageSays, usedIn } from './usage';

/** Magazyn agentów — dokładnie ten, który oddaje `createAgentsStore`. */
export type AgentsStore = ReturnType<typeof createAgentsStore>;

export interface AgentsScreenProps {
  /** Bez propsu ekran bierze swój prawdziwy magazyn, z propsem ten z testu. */
  store?: AgentsStore;
  /**
   * Ile workflow nazywa którego agenta — trzy stany, i wszystkie trzy niosą treść.
   *
   * `undefined` znaczy „przeczytaj katalog workflow sam" i tak jedzie produkcja. `null` znaczy
   * „NIE WIEM" i wtedy wiersz `used in …` nie rysuje się wcale. Mapa znaczy „policzone", a brak
   * klucza w niej to uczciwe zero.
   *
   * Osobne wejście, a nie pole magazynu agentów: to jest fakt o PLIKACH WORKFLOW, a magazyn
   * agentów trzyma bibliotekę agentów. Wstawienie go do `AgentsState` znaczyłoby, że sekcja
   * Agents ma drugie zdanie o zawartości cudzego katalogu (niezmiennik 13).
   *
   * Prop przyjmuje GOTOWĄ mapę, nie funkcję, i to jest wymuszone przez repo, nie wybrane:
   * `renderToStaticMarkup` nie odpala `useEffect`, więc czytnik podany propsem nigdy nie zdążyłby
   * się rozwiązać i żaden test nie zobaczyłby wiersza, który sprawdza.
   */
  usage?: Record<string, number> | null;
  /**
   * Agent otwarty w panelu na wejściu. Produkcja tego nie podaje — panel otwiera KLIK kafelka.
   *
   * Ten prop istnieje wyłącznie jako szew testowy i nie ma innego wołającego: w repo nie ma
   * jsdom, `renderToStaticMarkup` nigdy nie odpala `onClick`, więc bez niego markup panelu
   * zapisanego agenta — dziewięć pól z jego wartościami, `button-danger`, dwustopniowe pytanie
   * o usunięcie — nie da się w ogóle obejrzeć w teście. Szew, którego produkcja nie używa, jest
   * tu tańszy niż niesprawdzony panel: dokładnie ten panel nie montował się do 2026-08-18 dla
   * żadnego zapisanego agenta i nikt tego nie zauważył.
   */
  opened?: Agent;
}

/* Adapter dysku sekcji — PRAWDZIWY, od 2026-08-17.
 *
 * Do tego dnia stała tu zaślepka: `list` oddawał pustą tablicę, a `newId`, `save` i `remove`
 * odmawiały zdaniem „Loadout cannot reach the folder that holds agents yet". Jej komentarz
 * mówił, że `src/sections/agents/io.ts` nie istnieje i że warstwę IPC dowozi T-27.
 *
 * T-27 dowiozło. Plik istniał od kilkunastu godzin, eksportował dokładnie te cztery funkcje
 * i NIKT go nie wołał — jedynym miejscem w repo, które go importowało, był test. Sekcja przez
 * ten czas była trwale pusta, a Create odmawiał pod palcem, i nic tego nie zgłaszało: zaślepka
 * odpowiadająca „nic tam nie leży" czyta się dokładnie tak samo jak pusta biblioteka.
 *
 * Kształt modułu jest lustrem `AgentsIo`, więc podstawia się w całości. Adnotacja typu nie jest
 * ozdobą: to ona sprawdza, że moduł NADAL spełnia interfejs magazynu — funkcja usunięta po
 * tamtej stronie granicy przestaje się kompilować tutaj, zamiast odmawiać pod palcem. */
const DISK: AgentsIo = { ...Disk, list: Disk.listDefinitions };

/* Prawdziwy magazyn sekcji powstaje RAZ, przy wczytaniu modułu — magazyn budowany w ciele
 * komponentu gubiłby zawartość ekranu przy każdym przemontowaniu. */
const OWN_STORE = createAgentsStore(DISK);

/* Brzmienia vendorów czytane z JEDNEJ tabeli — tej, którą ma formularz. Do 2026-08-18 stała
 * tu druga kopia i jej własny komentarz nazywał to długiem: „naprawa jest jednoliniowa
 * i należy do właściciela tamtego pliku: `export const VENDORS`". Ta linia jest napisana. */
const VENDOR_SAYS: Readonly<Record<string, string>> = Object.fromEntries(
  VENDORS.map((option) => [option.value, option.label]),
);

/* Pięć przygaszonych tokenów tożsamości, `--id-1`…`--id-5` (DESIGN §3). Kolejność jest ta sama,
 * co w unii `Color` w `src/state/agents.ts`, i tak samo jak w makiecie: `clay` to `--id-3`,
 * dokładnie jak kwadrat Forge'a (`docs/mockup/index.html`, sekcja `agents`).
 *
 * TO NIE JEST KOLOR STANU. Cztery kolory stanu (`--accent --attend --fail --human`) są
 * nasycone i znaczą „teraz", „twoja kolej", „zepsute", „zrobił to człowiek". Tożsamość jest
 * PRZYGASZONA i nie ma prawa być z nimi pomylona — referencyjny poprzedni prototyp dawał agentowi Forge
 * dokładnie ten sam hex, co „wymaga uwagi" (DESIGN §3). Dlatego stan agenta jest w tej
 * aplikacji SŁOWEM w kolorze nasyconym, a kwadrat nie mówi o stanie nigdy. */
const ID_COLOUR: Readonly<Record<Color, string>> = {
  slate: 'bg-id-1',
  plum: 'bg-id-2',
  clay: 'bg-id-3',
  moss: 'bg-id-4',
  rose: 'bg-id-5',
};

/* Brzmienia z tabeli „We say / We never say" [T4 §8.1]. Nazwa z drutu (`look-only`) nie ma
 * prawa dojechać na ekran (niezmiennik 14). */
const FILE_ACCESS_SAYS: Readonly<Record<FileAccess, string>> = {
  'look-only': 'Look only',
  'ask-first': 'Ask first',
  'work-freely': 'Work freely',
};

const THINKING_SAYS: Readonly<Record<Thinking, string>> = {
  quick: 'Quick',
  balanced: 'Balanced',
  deep: 'Deep',
  deepest: 'Deepest',
};

/* Klasy komponentów z DESIGN §6. Wysokości idą po siatce 4px: 36px = `h-9` (button-primary),
 * 28px = `h-7` (button-quiet). */
const PRIMARY = 'h-9 rounded-sm bg-accent px-4 text-ui text-bg';
const QUIET = 'h-7 rounded-sm border border-line px-3 text-ui text-body';
/* `button-danger` z DESIGN §6: jak `button-secondary`, ale obrys `--fail-edge` i tekst `--fail`,
 * BEZ WYPEŁNIENIA. Akcja niszcząca ma być rozpoznawalna, a nie najbardziej rzucająca się
 * w oczy rzecz na ekranie. */
const DANGER = 'h-7 rounded-sm border border-fail-edge px-3 text-ui text-fail';
/* `chip`, wariant neutralny: vendor, model i głębokość myślenia są tożsamością agenta, a nie
 * jego stanem — nasycony kolor znaczy w tej aplikacji „twoja kolej" albo „teraz" (DESIGN §3). */
const CHIP =
  'h-5 shrink-0 rounded-pill border border-line bg-raised px-2 font-mono text-meta text-muted';
/* Kwadrat tożsamości ma 22 px — makieta, `.sqid{width:22px;height:22px}`. DESIGN.md podaje tu
 * dwie różne liczby (22 px w linii 127, 14 px w linii 243) i przy rozbieżności wygrywa makieta;
 * rozjazd jest zgłoszony człowiekowi. `size-5.5` to `calc(var(--spacing) * 5.5)`, czyli 22 px
 * na bazie 4px — nie literał (`checks/quick-tokens.sh`). */
const SQID =
  'grid size-5.5 shrink-0 place-items-center rounded-sm font-mono text-mono-strong text-ink';

/**
 * Nowy agent, zanim człowiek cokolwiek w nim wpisze.
 *
 * Dwie wartości są decyzjami, nie wypełniaczem: `fileAccess` jest najwęższy z trzech, bo prawo
 * do zmieniania plików ma dawać człowiek, a nie wartość domyślna; `id` jest puste, bo
 * identyfikator wybija mennica po stronie Rusta przy zapisie [T4 §5.1], a nie ekran — i to
 * PUSTE `id` jest umową z magazynem: `store.save` po nim rozpoznaje nowego agenta.
 */
function blankAgent(): Agent {
  return {
    schema: 1,
    id: '',
    name: '',
    summary: '',
    color: 'slate',
    instructions: '',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'look-only',
    giveUpAfterMinutes: 10,
    tools: 'everything',
    /* WŁĄCZONA, i to jest rozstrzygnięcie właściciela z 2026-08-23, nie wypełniacz. Powód stoi
     * przy `Agent::reaches_the_web` w Ruście: do wyłączonej domyślnej trzeba było TRAFIĆ,
     * a w jego bibliotece nie trafił nikt — 18 agentów, ani jeden z siecią. Dial zostaje przy
     * tym najwęższy z trzech: sieć nie daje ani jednego czasownika plikowego. */
    reachesTheWeb: true,
    skills: [],
    connections: [],
    writeResultsTo: '',
  };
}

/** Litera w kwadracie tożsamości. Pusta nazwa daje `?`, nie pusty kwadrat — kwadrat bez znaku
 * wygląda na awarię rysowania, a nie na agenta, któremu nie nadano jeszcze nazwy. */
function initial(name: string): string {
  return name.trim().slice(0, 1).toUpperCase() || '?';
}

/** `gives up after 20m`, a przy zerze prawda: limitu nie ma [T4 §4.3, reguła 1]. */
function giveUpSays(minutes: number): string {
  return minutes <= 0 ? 'no time limit' : `gives up after ${String(minutes)}m`;
}

export default function AgentsScreen({
  store = OWN_STORE,
  usage: usageProp,
  opened,
}: AgentsScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  /* Co jest OTWARTE w panelu — nowy szkic albo kopia zapisanego agenta. `null` znaczy, że nic
   * (niezmiennik 13). Stan jest lokalny, bo dotyczy ekranu, a nie tego, co leży na dysku.
   *
   * Panel dostaje KOPIĘ, nie agenta z listy: edycja mutująca wiersz magazynu pokazywałaby na
   * liście zmiany, których jeszcze nikt nie zapisał, a `Cancel` nie miałby do czego wrócić. */
  const [draft, setDraft] = useState<Agent | null>(opened ?? null);
  const [expanded, setExpanded] = useState(false);
  /* O co pytamy przed usunięciem. `null` znaczy, że o nic — jedno miejsce na to pytanie. */
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  /* Ile workflow nazywa którego agenta, przeczytane z dysku. `null` znaczy „NIE WIEM", i to jest
   * różnica, która decyduje o tym, czy wiersz `used in …` się w ogóle rysuje: `{}` znaczy
   * „policzone i zero". UI nie rysuje relacji, których nie ma w danych (niezmiennik 17),
   * a `used in 0 workflows` wypisane, kiedy katalogu workflow nie udało się przeczytać, jest
   * zdaniem nieprawdziwym — i to jest gorsze niż milczenie, bo wygląda na odpowiedź. */
  const [read, setRead] = useState<Record<string, number> | null>(null);
  const usage = usageProp === undefined ? read : usageProp;

  useEffect(() => {
    void store.getState().load();
  }, [store]);

  useEffect(() => {
    /* Wołający, który podał `usage`, już zna odpowiedź — drugie pytanie do dysku byłoby drugim
     * miejscem, z którego ona przychodzi. */
    if (usageProp !== undefined) return;

    /* `live` zamiast bezwarunkowego `setRead`: odmowa dysku po odmontowaniu sekcji ustawiałaby
     * stan komponentu, którego już nie ma. Odmowa NIE dostaje zdania na ekranie — brak liczby
     * jest tu uczciwszy niż liczba z niczego, a sama biblioteka agentów czyta się osobno
     * i ma własne zdanie odmowy. */
    let live = true;
    void readUsage()
      .then((counted) => {
        if (live) setRead(counted);
      })
      .catch(() => {
        if (live) setRead(null);
      });
    return () => {
      live = false;
    };
  }, [usageProp]);

  /* Jedna funkcja na całą sekcję i to jest cały sens niezmiennika 16: przycisk w nagłówku
   * i przycisk w zaproszeniu są dwoma wejściami do JEDNEJ ścieżki. Drugie kliknięcie nie
   * kasuje tego, co człowiek zdążył wpisać. */
  const startDraft = (): void => {
    setDraft((open) => open ?? blankAgent());
    setExpanded(false);
    setPendingDelete(null);
  };

  /* Otwiera ZAPISANEGO agenta. Kopia przez `structuredClone`, nie `{ ...agent }`: płytka kopia
   * współdzieliłaby `skills`, `connections` i `vendorOptions` z wierszem magazynu, więc pierwsza
   * zmiana listy w panelu przepisywałaby po cichu agenta na liście — ta sama pułapka, którą
   * opisuje `duplicate` w `src/state/agents.ts`. */
  const open = (agent: Agent): void => {
    setDraft(structuredClone(agent));
    setExpanded(false);
    setPendingDelete(null);
  };

  const save = (agent: Agent): void => {
    /* Panel zamyka się TYLKO wtedy, gdy dysk potwierdził. Nieudany zapis zostawia go otwartym
     * z tym, co człowiek wpisał, a zdanie odmowy stoi w magazynie i jest wyrenderowane niżej
     * (niezmiennik 4: ekran zgadza się z tym, co naprawdę leży na dysku). */
    void store
      .getState()
      .save(agent)
      .then((saved) => {
        if (saved) setDraft(null);
      });
  };

  const empty = state.agents.length === 0 && state.problems.length === 0;

  return (
    <section className="flex h-full flex-col">
      <header className="flex h-13 items-center gap-3 border-b border-line bg-panel px-4">
        <h1 className="text-title text-ink">Agents</h1>

        <button
          type="button"
          className={`ml-auto ${QUIET}`}
          onClick={() => {
            setImporting(true);
          }}
        >
          Import setup
        </button>

        {/* Licznik i przycisk w nagłówku żyją tylko wtedy, gdy jest co liczyć. Przy zerze to
            samo mówi zaproszenie niżej, a `0 saved` obok `No agents yet.` to ten sam fakt
            w dwóch miejscach (niezmiennik 13) — i druga kontrolka dodawania na ekranie,
            na którym DESIGN §6 przewiduje dokładnie jedną. Ten sam układ ma wylądowana lista
            workflow. */}
        {empty ? null : (
          <>
            {state.agents.length === 0 ? null : (
              <span className="font-mono text-mono text-muted">{`${String(state.agents.length)} saved`}</span>
            )}
            {state.problems.length === 0 ? null : (
              <span className="font-mono text-mono text-fail">{`${String(state.problems.length)} need attention`}</span>
            )}
            <button data-create type="button" className={PRIMARY} onClick={startDraft}>
              ＋ Create
            </button>
          </>
        )}
      </header>

      {/* Zdanie, które napisał dysk. Stoi POD nagłówkiem i nad listą, bo dotyczy całej sekcji,
          a nie jednej kontrolki — a przy zapisie panel jest wtedy dalej otwarty i człowiek
          widzi obok siebie to, co wpisał, i powód, dla którego to nie weszło. */}
      {state.refusal === null ? null : (
        <div
          data-refusal
          role="alert"
          className="flex items-start gap-3 border-b border-fail-edge bg-fail-soft px-4 py-2"
        >
          <p className="text-body text-fail">{state.refusal}</p>
          <button
            type="button"
            className={`ml-auto ${QUIET}`}
            onClick={() => {
              store.getState().dismiss();
            }}
          >
            Dismiss
          </button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        <div className="min-h-0 flex-1 overflow-auto p-4">
          {empty ? (
            <div className="flex h-full flex-col items-center justify-center gap-3">
              <span className="flex size-8 items-center justify-center rounded-md border border-dashed border-line-strong text-muted">
                ◇
              </span>
              {/* `data-empty` siedzi na elemencie, który niesie SAMO zdanie — nie na ramce
                  z zaproszeniem. Tak samo robi `src/App.tsx` i z tego samego powodu: treścią
                  tak oznaczonego elementu ma być zdanie, a nie „◇ zdanie ＋ Create". */}
              <p data-empty className="text-ink">
                No agents yet.
              </p>
              <p className="text-muted">Add one, and a step in any workflow can be handed to it.</p>
              <button data-create type="button" className={PRIMARY} onClick={startDraft}>
                ＋ Create
              </button>
            </div>
          ) : (
            <ul className="grid grid-cols-2 gap-3">
              {state.agents.map((agent) => (
                <li key={agent.id}>
                  {/* KAFELEK JEST PRZYCISKIEM, tak jak w makiecie (`<button class="tile">`).
                      Do 2026-08-18 był `<li>` bez handlera i zapisany agent zostawał na liście
                      na zawsze: panel montował się wyłącznie dla nowego szkicu, więc literówki
                      w instrukcjach nie dało się poprawić z okna. */}
                  <button
                    data-agent={agent.id}
                    type="button"
                    className="flex w-full flex-col gap-2 rounded-md border border-line bg-panel p-3 text-left hover:border-line-strong"
                    onClick={() => {
                      open(agent);
                    }}
                  >
                    <div className="flex items-center gap-2">
                      <span
                        data-identity={agent.color}
                        className={`${SQID} ${ID_COLOUR[agent.color]}`}
                        aria-hidden="true"
                      >
                        {initial(agent.name)}
                      </span>
                      <h2 className="text-subhead text-ink">{agent.name}</h2>
                      {/* Na czym ten agent biegnie i którym modelem. Obaj vendorzy są pierwszej
                          kategorii (D3), więc etykieta stoi przy KAŻDYM agencie, a nie tylko
                          przy tym, który odstaje od domyślnego. */}
                      <span
                        className={CHIP}
                      >{`${VENDOR_SAYS[agent.runsWith] ?? agent.runsWith} · ${agent.model}`}</span>
                      <span className={CHIP}>{THINKING_SAYS[agent.thinking]}</span>
                    </div>
                    <p className="text-note text-muted">{agent.summary}</p>
                    <div className="flex gap-3 border-t border-line pt-2 font-mono text-meta text-muted">
                      <span>{FILE_ACCESS_SAYS[agent.fileAccess]}</span>
                      <span>{giveUpSays(agent.giveUpAfterMinutes)}</span>
                      {/* Trzeci wiersz makiety rysuje się TYLKO wtedy, gdy katalog workflow
                          został naprawdę przeczytany — patrz `usage` wyżej i `usage.ts`. */}
                      {usage === null ? null : <span>{usageSays(usedIn(usage, agent.id))}</span>}
                    </div>
                  </button>
                </li>
              ))}
              {state.problems.map((problem) => (
                <li
                  key={problem.fileName}
                  data-definition-problem={problem.fileName}
                  className="flex flex-col gap-2 rounded-md border border-fail-edge bg-panel p-3"
                >
                  <h2 className="text-subhead text-ink">{problem.fileName}</h2>
                  <p className="text-body text-muted">{problemSays(problem)}</p>
                </li>
              ))}
            </ul>
          )}
        </div>

        {draft === null ? null : (
          <aside className="min-h-0 w-83 overflow-auto border-l border-line bg-panel p-4">
            <div className="flex items-center gap-2 pb-3">
              <h2 className="text-heading text-ink">
                {draft.id === '' ? 'New agent' : draft.name}
              </h2>
              <button
                type="button"
                className={`ml-auto ${QUIET}`}
                onClick={() => {
                  setDraft(null);
                  setPendingDelete(null);
                }}
              >
                Cancel
              </button>
            </div>

            <AgentForm
              value={draft}
              expanded={expanded}
              onChange={setDraft}
              onToggleMore={() => {
                setExpanded((wasOpen) => !wasOpen);
              }}
              onSave={() => {
                save(draft);
              }}
            />

            {/* Kopiowanie i usuwanie dotyczą agenta, który JUŻ leży na dysku, więc dla nowego
                szkicu tych kontrolek nie ma. Przycisk, który miałby usunąć plik, którego nie ma,
                jest kontrolką bez skutku (niezmiennik 16). */}
            {draft.id === '' ? null : (
              <div className="mt-3 flex flex-col gap-2 border-t border-line pt-3">
                {pendingDelete === draft.id ? (
                  /* POTWIERDZENIE JEST RENDEREM, nie `window.confirm`. Dialog przeglądarki
                     blokuje webview i zabiera całą sesję — przy oknie Tauri nie ma go czym
                     odblokować. Zdanie nazywa agenta, bo „Are you sure?" nie mówi, o co pytamy,
                     a panel bywa otwarty od kilku minut. */
                  <>
                    <p data-confirm-delete className="text-body text-ink">
                      {`Delete ${draft.name}? Steps that use it will have nothing to run.`}
                    </p>
                    <div className="flex items-center gap-2">
                      <button
                        data-delete-confirm
                        type="button"
                        className={DANGER}
                        onClick={() => {
                          const doomed = draft.id;
                          setPendingDelete(null);
                          setDraft(null);
                          void store.getState().delete(doomed);
                        }}
                      >
                        Delete
                      </button>
                      <button
                        type="button"
                        className={QUIET}
                        onClick={() => {
                          setPendingDelete(null);
                        }}
                      >
                        Keep it
                      </button>
                    </div>
                  </>
                ) : (
                  <div className="flex items-center gap-2">
                    <button
                      data-duplicate
                      type="button"
                      className={QUIET}
                      onClick={() => {
                        void store.getState().duplicate(draft.id);
                      }}
                    >
                      Duplicate
                    </button>
                    <button
                      data-delete
                      type="button"
                      className={`ml-auto ${DANGER}`}
                      onClick={() => {
                        setPendingDelete(draft.id);
                      }}
                    >
                      Delete
                    </button>
                  </div>
                )}
              </div>
            )}
          </aside>
        )}
      </div>
      {importing ? (
        <ImportSetup
          onClose={() => {
            setImporting(false);
          }}
          onImported={() => {
            setImporting(false);
            void store.getState().load();
          }}
        />
      ) : null}
    </section>
  );
}
