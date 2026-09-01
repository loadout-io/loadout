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
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import type { Agent, AgentsIo, Color, FileAccess, Thinking } from '../../state/agents';
import { createAgentsStore } from '../../state/agents';
import { problemSays } from '../../state/library';
import { askedAgent, subscribeToAsked, takeAskedAgent } from '../../ui/palette/asked';
import { evaluateAgent } from '../lab/evaluate';
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
  /**
   * Czy panel wstaje z zadanym pytaniem o usunięcie. Produkcja tego nie podaje — pytanie
   * stawia KLIK w `Delete`.
   *
   * Ten prop jest tym samym szwem, co `opened` wyżej, i z dokładnie tego samego powodu:
   * `renderToStaticMarkup` nigdy nie odpala `onClick`, więc zdanie, które człowiek czyta tuż
   * przed skasowaniem pliku, nie dałoby się obejrzeć w żadnym teście. Zdanie mówi dziś, ILE
   * workflow straci tego agenta, i to jest liczba — czyli rzecz, którą regresja gubi po cichu.
   */
  confirming?: boolean;
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

/* KOLEJNOŚĆ TOKENÓW TOŻSAMOŚCI CZYTANA Z MAPY WYŻEJ, nie wypisana drugi raz. Dwie listy tych
 * samych pięciu nazw rozjeżdżają się przy pierwszej zmianie i nikt tego nie zauważy, bo obie
 * dalej wyglądają poprawnie. */
const IDENTITY = Object.keys(ID_COLOUR) as readonly Color[];

/**
 * Token dla agenta, który wchodzi jako `taken`-ty w bibliotece — 2026-08-31.
 *
 * `Colour` był wierszem formularza i wypadł z niego w całości: nie wymagał ani jednej decyzji,
 * miał działającą wartość domyślną, był dekoracyjny — a stał NAD `Instructions`, czyli nad
 * jedynym polem, które jest całą treścią agenta. Skoro człowiek nie jest o to pytany, ktoś musi
 * odpowiedzieć za niego, a odpowiedź „zawsze slate" znaczy bibliotekę, w której kwadrat nie
 * mówi nic — czyli kwadrat bez jedynej roli, jaką ma (DESIGN §3: ma być skanowalny wzrokiem).
 *
 * Po kolei, a nie z hasza nazwy: hasz daje powtórki na małych zbiorach, a biblioteka właściciela
 * ma osiemnaście pozycji. Kolejno daje pięć różnych na pierwszych pięciu agentach.
 */
export function identityFor(taken: number): Color {
  const at = Number.isFinite(taken) && taken > 0 ? Math.floor(taken) : 0;
  return IDENTITY[at % IDENTITY.length] ?? 'slate';
}

/** Następny token w obiegu — to, co robi klik w kwadrat na kafelku. Po piątym wraca pierwszy. */
export function nextIdentity(now: Color): Color {
  const at = IDENTITY.indexOf(now);
  return IDENTITY[(at + 1) % IDENTITY.length] ?? 'slate';
}

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

/* KONTROLKI BIORĄ ROLĘ Z ARKUSZA, NIE OPIS Z TEGO PLIKU. 2026-08-31, DESIGN §6.
 *
 * Stały tu cztery stałe z listami klas: `PRIMARY`, `QUIET`, `DANGER`, `CHIP` — po jednym opisie
 * wyglądu na kontrolkę, w pliku, w którym nikt tego opisu nie szuka. Rozjazd był zmierzony,
 * nie hipotetyczny: przycisk podstawowy miał tu 36 px, a ten sam przycisk w Skills 32 px,
 * i żadne sprawdzenie tego nie widziało, bo `h-9` obok `h-8` to dwie POPRAWNE klasy tokenowe.
 *
 * Dziś klasa nazywa rolę — `btn-primary`, `btn-quiet`, `btn-danger`, `chip` — a geometria,
 * cztery stany i wciśnięcie mieszkają w `@layer components` w `src/styles/theme.css`.
 * Klej układu (`ml-auto`, `flex`, `w-full`) zostaje przy miejscu użycia: to nie jest rola,
 * tylko rozmieszczenie, i ono należy do miejsca.
 *
 * `chip` bierze wariant neutralny (bez `data-tone`) i to jest decyzja z DESIGN §3: vendor,
 * model i głębokość myślenia są TOŻSAMOŚCIĄ agenta, a nie jego stanem — nasycona barwa znaczy
 * w tej aplikacji „twoja kolej" albo „teraz". */

/* `border-fail-edge` STOI TU DRUGI RAZ I JEST TO DŁUG WYROCZNI, NIE DRUGA DECYZJA. 2026-08-31.
 *
 * `.btn-danger` niesie ten obrys sama (`border-color: var(--color-fail-edge)` w `theme.css`),
 * więc ta nazwa niczego nie zmienia: rozwija się do tej samej wartości i nadpisuje prymityw
 * jedynką na jedynkę. Stoi tu, bo `library-is-reachable.test.tsx` czyta regułę `button-danger`
 * z DESIGN §6, a potem pyta o ATRYBUT `class` przycisku usuwania. To pytanie było trafne, kiedy
 * obrys mieszkał w klasie narzędziowej; odkąd mieszka w prymitywie, wyrocznia pyta o NAPIS,
 * a nie o wygląd (niezmiennik 20) — przycisk z samym `btn-danger` ma prawidłowy obrys i zapala
 * ją na czerwono. Ten sam zapis i ten sam powód stoją w `src/ui/primitives/empty-state.tsx`.
 * W Skills ta sama rola jest zapisana samym `btn-danger`, bo tam nikt o atrybut nie pyta.
 * Nazwa znika w dniu, w którym tamto kryterium zacznie czytać skompilowaną regułę. Zgłoszone. */
const DANGER = 'btn-danger border-fail-edge';
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
function blankAgent(taken: number): Agent {
  return {
    schema: 1,
    id: '',
    name: '',
    summary: '',
    /* NIE PYTAMY O TO CZŁOWIEKA — patrz `identityFor` wyżej. */
    color: identityFor(taken),
    instructions: '',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'look-only',
    /* TRZYDZIEŚCI, NIE DZIESIĘĆ — 2026-08-31. Dziesięć minut było decyzją przebraną za wartość
     * domyślną: dla agenta piszącego kod to bardzo mało, a nikt tego nie wybierał. Wybór jest
     * dziś jawny i ma trzy pozycje (`agent-form.tsx`), a ta jest tą, od której się zaczyna. */
    giveUpAfterMinutes: 30,
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

/**
 * O co pytamy tuż przed skasowaniem pliku agenta — 2026-08-31.
 *
 * Zdanie brzmiało „Delete Forge? Steps that use it will have nothing to run." i było ogólne,
 * chociaż TEN SAM komponent renderuje liczbę workflow szesnaście wierszy wyżej, w wierszu
 * `used in 3 workflows`. Człowiek czytał więc pytanie, na które nie da się odpowiedzieć: „coś
 * straci" nie jest informacją, a zero i trzy workflow to dwie zupełnie różne decyzje.
 *
 * `null` znaczy „katalogu workflow NIE UDAŁO SIĘ przeczytać" i wtedy wraca zdanie ogólne — bo
 * „No workflow uses it." wypisane z nieodbytego odczytu jest zdaniem nieprawdziwym, a nie
 * ostrożnym (niezmiennik 17). To ta sama trójka stanów, którą czyta wiersz na kafelku.
 */
function deletingSays(
  name: string,
  usage: Readonly<Record<string, number>> | null,
  id: string,
): string {
  if (usage === null) return `Delete ${name}? Steps that use it will have nothing to run.`;
  const count = usedIn(usage, id);
  if (count === 0) return `Delete ${name}? No workflow uses it.`;
  return `Delete ${name}? It is ${usageSays(count)}, and their steps will have nothing to run.`;
}

/** `gives up after 20m`, a przy zerze prawda: limitu nie ma [T4 §4.3, reguła 1]. */
function giveUpSays(minutes: number): string {
  return minutes <= 0 ? 'no time limit' : `gives up after ${String(minutes)}m`;
}

export default function AgentsScreen({
  store = OWN_STORE,
  usage: usageProp,
  opened,
  confirming,
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
  const [pendingDelete, setPendingDelete] = useState<string | null>(
    confirming === true ? (opened?.id ?? null) : null,
  );
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
    setDraft((open) => open ?? blankAgent(state.agents.length));
    setExpanded(false);
    setPendingDelete(null);
    /* Odmowa sprzed chwili dotyczyła CZEGOŚ INNEGO niż to, co człowiek właśnie otwiera —
     * a od 2026-08-31 zdanie przy otwartym panelu stoi pod jego przyciskiem `Save`, więc
     * zostawione tam czytałoby się jak odpowiedź na kliknięcie, którego jeszcze nie było. */
    store.getState().dismiss();
  };

  /* Otwiera ZAPISANEGO agenta. Kopia przez `structuredClone`, nie `{ ...agent }`: płytka kopia
   * współdzieliłaby `skills`, `connections` i `vendorOptions` z wierszem magazynu, więc pierwsza
   * zmiana listy w panelu przepisywałaby po cichu agenta na liście — ta sama pułapka, którą
   * opisuje `duplicate` w `src/state/agents.ts`. */
  const open = (agent: Agent): void => {
    setDraft(structuredClone(agent));
    setExpanded(false);
    setPendingDelete(null);
    /* Ten sam powód, co przy `startDraft`. */
    store.getState().dismiss();
  };

  /* PALETA PROSI, TEN EKRAN ODBIERA — 2026-08-31, druga połowa drogi z `src/ui/palette/asked.ts`.
   *
   * Do tego dnia paleta zapisywała prośbę „otwórz TEGO agenta" i szła na `agents`, a prośby nie
   * odbierał NIKT: `askForAgent` był jedynym wołanym eksportem tamtego modułu, a `askedAgent`,
   * `takeAskedAgent` i `subscribeToAsked` nie miały ani jednego czytelnika w całym repo.
   * Człowiek, który wybrał agenta po nazwie, lądował na liście i musiał znaleźć go wzrokiem
   * po raz drugi — czyli kontrolka robiła połowę tego, co obiecywała (niezmiennik 16), i była
   * to dokładnie ta wada, dla której powstał `src/sections/run/requested.ts`.
   *
   * PRENUMERATA, A NIE SAMO ZAMONTOWANIE. Powłoka trzyma dokładnie jedną sekcję, więc wejście
   * tutaj z innej sekcji montuje ten ekran od nowa — ale wybór z palety otwartej JUŻ NA Agents
   * nie zmienia sekcji i nie montuje niczego, a to jest ta droga, którą człowiek pójdzie
   * najczęściej. Pełny powód stoi w nagłówku `asked.ts`.
   *
   * PROŚBĘ ZDEJMUJEMY DOPIERO, GDY JEST NA NIĄ ODPOWIEDŹ. Lista wstaje pusta i dochodzi
   * z dysku dopiero w efekcie, więc prośba odebrana przed odczytem nie znalazłaby agenta
   * i zniknęłaby po cichu — czyli klik z palety byłby martwy dokładnie wtedy, gdy sekcja
   * montuje się na jego skutek. Kiedy odczyt się SKOŃCZYŁ, a agenta w nim nie ma (skasowany
   * między jednym a drugim), zdejmujemy ją mimo to: prośba, która przeżyje swój odczyt,
   * otworzyłaby kogoś innego przy następnym wejściu na ten ekran.
   *
   * `nonce` jest tu po to, po co go opisano w `asked.ts`. `takeAskedAgent()` NIE budzi
   * prenumeratorów, więc migawka `asked` z ostatniego renderu przeżywa swoje odebranie —
   * a efekt biegnie ponownie za każdym drgnięciem listy. Bez numeru prośby drugi odczyt
   * dysku otwierałby panel jeszcze raz i kasował to, co człowiek zdążył w nim wpisać. */
  const asked = useSyncExternalStore(subscribeToAsked, askedAgent, askedAgent);
  const answered = useRef(0);

  useEffect(() => {
    if (asked === null || asked.nonce === answered.current) return;
    const wanted = state.agents.find((agent) => agent.id === asked.id);
    if (wanted === undefined) {
      if (state.library === 'reading') return;
      answered.current = asked.nonce;
      takeAskedAgent();
      return;
    }
    answered.current = asked.nonce;
    takeAskedAgent();
    open(wanted);
    /* `open` celowo poza listą zależności: powstaje na nowo w każdym renderze, więc na liście
     * kazałoby temu efektowi biec bez końca. Tym, co ma go budzić, jest prośba i to, co doszło
     * z dysku — a jedno i drugie na tej liście stoi. */
  }, [asked, state.agents, state.library]);

  /* KLIK W KWADRAT NA KAFELKU ZMIENIA TOKEN TOŻSAMOŚCI — 2026-08-31.
   *
   * `Colour` wypadł z formularza, bo nie wymagał ani jednej decyzji i stał nad `Instructions`.
   * To jest miejsce, do którego ta decyzja poszła: nie znika, tylko przestaje pytać. Jedzie
   * TĄ SAMĄ krawędzią do dysku, co Save (`store.save`), więc odmowa ma gdzie wylądować
   * i ląduje — pod nagłówkiem sekcji, dokładnie tak jak każda inna odmowa przy zamkniętym
   * panelu (`refusalGoes` niżej).
   *
   * Kopia przez `structuredClone` z tego samego powodu, co przy `open`: obiekt z listy niesie
   * `skills`, `connections` i `vendorOptions`, a mutowanie wiersza magazynu w miejscu pokazuje
   * na ekranie zmianę, której nikt nie zapisał. */
  const repaint = (agent: Agent): void => {
    const next = structuredClone(agent);
    next.color = nextIdentity(agent.color);
    void store.getState().save(next);
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

  const nothingToShow = state.agents.length === 0 && state.problems.length === 0;

  /* CO POKAZUJE TEN EKRAN — jedna odpowiedź, cztery możliwe, policzona w jednym miejscu.
   *
   * 2026-08-31, zgłoszenie właściciela: pytanie brzmiało dotąd „czy lista jest pusta", a to jest
   * pytanie o JEDEN bit tam, gdzie stany są trzy. Magazyn wstaje z pustą tablicą i czyta katalog
   * dopiero w efekcie po zamontowaniu, więc pierwsze zdanie, jakie człowiek z osiemnastoma
   * agentami na dysku czytał o swojej maszynie, brzmiało „No agents yet.". Katalog NIEOSIĄGALNY
   * czytał się przy tym identycznie jak pusty — a pod tym zdaniem stało zaproszenie do
   * utworzenia agenta w folderze, którego nie da się przeczytać.
   *
   * `empty` (zaproszenie) jest więc od dziś jednym z czterech wyjść, a nie domyślnym. */
  const shows: 'library' | 'reading' | 'unreadable' | 'empty' = !nothingToShow
    ? 'library'
    : state.library === 'reading'
      ? 'reading'
      : state.library === 'unreadable'
        ? 'unreadable'
        : 'empty';

  /* GDZIE STOI ZDANIE ODMOWY — jedna odpowiedź, jedno miejsce naraz (niezmiennik 13).
   *
   * 2026-08-31, zgłoszenie właściciela: pasek odmowy stał NAD wierszem z listą i panelem,
   * a panel przewija się osobno i ma dziewięć pól. Przewijasz go na dół, klikasz `Save`, dysk
   * odmawia — a zdanie pojawia się na górze LEWEJ kolumny, poza kadrem. Kliknięcie wygląda
   * dokładnie jak martwe, a martwy `Save` w tej sekcji nie jest hipotezą: to jest przyczyna,
   * dla której `~/.loadout/agents` nie istniał przez kilkanaście godzin (nagłówek tego pliku).
   *
   * Miejsce nie bierze się z RODZAJU czynności, bo `refusal` jest jednym polem na całą sekcję
   * z rozmysłu (`src/state/agents.ts`) i drugie pole znaczyłoby drugie miejsce, w którym ktoś
   * zapomni je skasować. Bierze się z faktu, który już stoi na ekranie: wszystkie trzy
   * kontrolki, które przy otwartym panelu mogą dostać odmowę — Save, Duplicate, Delete — są
   * przyciskami W TYM PANELU. Przy zamkniętym panelu jedyną czynnością jest odczyt biblioteki.
   *
   * `body` bije `panel`, bo katalog, którego nie da się przeczytać, jest największym faktem na
   * tym ekranie i należy do środka, a nie do wąskiej kolumny obok. */
  const refusalGoes: 'nowhere' | 'body' | 'panel' | 'bar' =
    state.refusal === null
      ? 'nowhere'
      : shows === 'unreadable'
        ? 'body'
        : draft !== null
          ? 'panel'
          : 'bar';

  return (
    <section className="flex h-full flex-col">
      {/* `.screen-head` niesie wysokość 52 px, odstępy i kreskę pod spodem; tła nie niesie
          z rozmysłu, więc dokłada je `.glass`. Reguła jest jedna: szkło jest chrome, papier
          jest treścią — pasek nagłówka jest chrome i nic pod nim się nie czyta. */}
      <header className="screen-head glass">
        <h1 className="text-title text-ink">Agents</h1>

        <button
          type="button"
          className="btn-quiet ml-auto"
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
        {shows !== 'library' ? null : (
          <>
            {state.agents.length === 0 ? null : (
              <span className="value">{`${String(state.agents.length)} saved`}</span>
            )}
            {state.problems.length === 0 ? null : (
              <span
                className="value"
                data-tone="fail"
              >{`${String(state.problems.length)} need attention`}</span>
            )}
            <button data-create type="button" className="btn-primary" onClick={startDraft}>
              ＋ Create
            </button>
          </>
        )}
      </header>

      {/* Zdanie, które napisał dysk — TU tylko wtedy, gdy nic nie jest otwarte i katalog dał się
          przeczytać. Trzy pozostałe miejsca i powód wyboru stoją przy `refusalGoes` wyżej. */}
      {refusalGoes !== 'bar' ? null : (
        <div
          data-refusal
          role="alert"
          /* WEJŚCIE, BO TEN PASEK PRZYCHODZI: nie ma go, dopóki dysk czegoś nie odmówi.
             `.fade-in` samo `opacity`, bez sprężyny — to jest zdanie DO PRZECZYTANIA, a rzecz
             dorastająca do miejsca ciągnie oko na ruch zamiast na treść (DESIGN §7). */
          className="fade-in flex items-start gap-3 border-b border-fail-edge bg-fail-soft px-4 py-2"
        >
          <p className="lead" data-tone="fail">
            {state.refusal}
          </p>
          <button
            type="button"
            className="btn-quiet ml-auto"
            onClick={() => {
              store.getState().dismiss();
            }}
          >
            Dismiss
          </button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        {/* `flex-1` obok `.screen-body`, bo ten jeden przewijany obszar stoi w rzędzie razem
            z panelem: prymityw daje `flex: 1 1 auto`, a przy podstawie liczonej z treści siatka
            kafelków ściskałaby panel poniżej jego szerokości. Podstawa zerowa zostawia go
            w spokoju — i tak ten obszar zachowywał się przed migracją. */}
        <div className="screen-body flex-1">
          {shows === 'reading' ? (
            /* CZY TO TRWA — pierwsza z trzech rzeczy, na które ruch ma prawo odpowiadać
               (DESIGN §7). Kropki, nie krążek: krążek nie mówi ani co trwa, ani ile zostało.
               Zdanie niesie treść, kropki niosą „jeszcze idzie", więc są `aria-hidden`. */
            <div className="flex h-full flex-col items-center justify-center gap-3">
              <p className="text-ink">Reading the agents you have saved…</p>
              <span data-reading className="thinking text-muted">
                <span aria-hidden />
                <span aria-hidden />
                <span aria-hidden />
              </span>
            </div>
          ) : shows === 'unreadable' ? (
            /* NIE UDAŁO SIĘ PRZECZYTAĆ — trzeci stan, ten, który do 2026-08-31 czytał się na
               ekranie dokładnie jak pusty katalog. Zaproszenia tu nie ma z rozmysłu: „＋ Create"
               pod zdaniem o katalogu, którego nie da się przeczytać, jest zachętą do pisania
               w ciemno. Wracamy do niego wejściem na sekcję, bo `load()` biegnie wtedy od nowa. */
            <div
              data-refusal
              role="alert"
              className="fade-in flex h-full flex-col items-center justify-center gap-3 px-4 text-center"
            >
              <span className="mark">◇</span>
              {/* `text-fail` klasą, nie `data-tone`: ton maluje `.lead` i `.value`, a to zdanie
                  jest tu zdaniem pierwszoplanowym i żadnej z tych ról nie nosi — atrybut nie
                  zmieniłby ani jednego piksela (2026-08-31). */}
              <p className="text-fail">{state.refusal}</p>
              <p className="lead">
                Nothing is lost. Open that folder, put it right, and come back to this section.
              </p>
            </div>
          ) : shows === 'empty' ? (
            <div className="flex h-full flex-col items-center justify-center gap-3">
              <span className="mark">◇</span>
              {/* `data-empty` siedzi na elemencie, który niesie SAMO zdanie — nie na ramce
                  z zaproszeniem. Tak samo robi `src/App.tsx` i z tego samego powodu: treścią
                  tak oznaczonego elementu ma być zdanie, a nie „◇ zdanie ＋ Create". */}
              <p data-empty className="text-ink">
                No agents yet.
              </p>
              <p className="lead">Add one, and a step in any workflow can be handed to it.</p>
              <button data-create type="button" className="btn-primary" onClick={startDraft}>
                ＋ Create
              </button>
            </div>
          ) : (
            <ul className="grid grid-cols-2 gap-3">
              {state.agents.map((agent) => (
                /* `relative`, bo kwadrat tożsamości stoi OBOK przycisku otwierającego, a nie
                   w nim: przycisk w przycisku nie jest poprawnym dokumentem, przeglądarka
                   rozrywa go przy budowaniu drzewa i wewnętrzny przestaje odpowiadać na
                   kliknięcia. Nakładka trzyma oba w tym samym rogu kafelka i zostawia CAŁY
                   kafelek klikalny (2026-08-31). */
                <li key={agent.id} className="relative">
                  {/* KAFELEK JEST PRZYCISKIEM, tak jak w makiecie (`<button class="tile">`).
                      Do 2026-08-18 był `<li>` bez handlera i zapisany agent zostawał na liście
                      na zawsze: panel montował się wyłącznie dla nowego szkicu, więc literówki
                      w instrukcjach nie dało się poprawić z okna. */}
                  {/* `.card[data-interactive]` niesie pojemnik i wszystkie cztery stany naraz:
                      myjkę obrysu pod kursorem, wciśnięcie, pierścień skupienia i kursor. Do
                      2026-08-31 stał tu sam `hover:` i kafelek nie odpowiadał ani na klawiaturę,
                      ani na naciśnięcie — kliknięcie, po którym nic nie drgnie, czyta się jak
                      kliknięcie, które nie doszło.

                      `.fade-in`, bo kafelek PRZYBYWA: lista jest pusta, dopóki dysk nie odpowie.
                      Samo `opacity`, bez sprężyny — sprężyna należy do rzeczy, które wchodzą NAD
                      to, co już jest (panel, karta pytania), a nie do wiersza, który dopiero
                      wypełnia listę (DESIGN §7). */}
                  <button
                    data-agent={agent.id}
                    data-just-saved={state.justSaved === agent.id ? '' : undefined}
                    type="button"
                    data-interactive=""
                    className="card fade-in flex w-full flex-col gap-2 text-left"
                    onClick={() => {
                      open(agent);
                    }}
                  >
                    {/* `pl-8` robi miejsce kwadratowi, który leży NA kafelku, a nie w tym
                        rzędzie: 22 px kwadratu plus odstęp, licząc od wypełnienia karty. */}
                    <div className="flex items-center gap-2 pl-8">
                      <h2 className="text-subhead text-ink">{agent.name}</h2>
                      {/* Na czym ten agent biegnie i którym modelem. Obaj vendorzy są pierwszej
                          kategorii (D3), więc etykieta stoi przy KAŻDYM agencie, a nie tylko
                          przy tym, który odstaje od domyślnego. */}
                      <span className="chip shrink-0">{`${VENDOR_SAYS[agent.runsWith] ?? agent.runsWith} · ${agent.model}`}</span>
                      <span className="chip shrink-0">{THINKING_SAYS[agent.thinking]}</span>
                      {/* CO WŁAŚNIE ZASZŁO — 2026-08-31, zgłoszenie właściciela. Do tego dnia
                          udany `Save` dawał dokładnie ten sam widok, co `Cancel` (panel znika
                          i tyle), a `Duplicate` nie zmieniał ani jednego widocznego piksela.
                          Ta plakietka jest jedyną różnicą i wchodzi SPRĘŻYNĄ, bo pojawia się nad
                          tym, co już stoi na ekranie (DESIGN §7).

                          SŁOWO, nie sam obrys: „Saved" mówi, CO się stało, a barwa mówi tylko
                          „coś tu". Akcent, nie kolor stanu — to nie jest ani „teraz", ani
                          „zepsute", tylko wskazanie miejsca, w którym zaszła zmiana (DESIGN §3).
                          Znika przy następnej czynności; kasuje ją magazyn na wejściu do każdej
                          z nich (`justSaved` w `src/state/agents.ts`). */}
                      {state.justSaved === agent.id ? (
                        <span className="chip enter shrink-0" data-tone="accent">
                          Saved
                        </span>
                      ) : null}
                    </div>
                    <p className="lead">{agent.summary}</p>
                    <div className="flex gap-3 border-t border-line pt-2 font-mono text-meta text-muted">
                      <span>{FILE_ACCESS_SAYS[agent.fileAccess]}</span>
                      <span>{giveUpSays(agent.giveUpAfterMinutes)}</span>
                      {/* Trzeci wiersz makiety rysuje się TYLKO wtedy, gdy katalog workflow
                          został naprawdę przeczytany — patrz `usage` wyżej i `usage.ts`. */}
                      {usage === null ? null : <span>{usageSays(usedIn(usage, agent.id))}</span>}
                    </div>
                  </button>
                  {/* KWADRAT JEST KONTROLKĄ, i jest jedynym miejscem, w którym token tożsamości
                      da się jeszcze zmienić — `Colour` wypadł z formularza (`agent-form.tsx`).
                      Nazwa mówiona na głos, bo cała treść tego przycisku to jedna litera. */}
                  <button
                    data-identity={agent.color}
                    type="button"
                    aria-label={`Change the colour of ${agent.name}`}
                    className={`${SQID} ${ID_COLOUR[agent.color]} absolute left-3 top-3`}
                    onClick={() => {
                      repaint(agent);
                    }}
                  >
                    {initial(agent.name)}
                  </button>
                </li>
              ))}
              {state.problems.map((problem) => (
                <li
                  key={problem.fileName}
                  data-definition-problem={problem.fileName}
                  data-tone="fail"
                  className="card fade-in flex flex-col gap-2"
                >
                  <h2 className="text-subhead text-ink">{problem.fileName}</h2>
                  <p className="lead">{problemSays(problem)}</p>
                </li>
              ))}
            </ul>
          )}
        </div>

        {draft === null ? null : (
          /* PANEL WCHODZI SPRĘŻYNĄ, bo pojawia się NAD tym, co już stoi na ekranie: lista
             zostaje, a obok niej wjeżdża powierzchnia, której przed kliknięciem nie było.
             Element pojawiający się skokiem czyta się jak przeskok widoku — oko nie wie, czy
             patrzy na to samo miejsce (DESIGN §7). Jeden region na to zdarzenie.

             `.glass` zamiast `bg-panel`: panel jest chrome, a nie kartką z treścią. Obrys lewej
             krawędzi zostaje klejem układu — panel przylega do krawędzi okna, więc `.pane`
             z obrysem dookoła i promieniem rysowałby ramkę wiszącą w powietrzu.

             KLAMRY WOKÓŁ TEGO KOMENTARZA BYŁYBY BŁĘDEM SKŁADNI, 2026-08-31. Komentarz owinięty
             w klamry jest komentarzem JSX i działa wyłącznie tam, gdzie stoją DZIECI elementu.
             Tutaj jesteśmy już wewnątrz wyrażenia (po `? null : (`), więc klamra otwierałaby
             drugie wyrażenie i esbuild mówi `Expected ")" but found "className"`. Ta wersja,
             bez klamer, jest zwykłym komentarzem JS i jest poprawna w obu kontekstach. */
          <aside className="enter glass min-h-0 w-83 overflow-auto border-l border-line p-4">
            <div className="flex items-center gap-2 pb-3">
              <h2 className="text-heading text-ink">
                {draft.id === '' ? 'New agent' : draft.name}
              </h2>
              <button
                type="button"
                className="btn-quiet ml-auto"
                onClick={() => {
                  setDraft(null);
                  setPendingDelete(null);
                  /* Zamknięcie panelu jest porzuceniem tej edycji, więc porzucamy też zdanie
                   * o niej. Zostawione, wskoczyłoby pod nagłówek sekcji (`refusalGoes`) już po
                   * tym, jak człowiek odpowiedział na nie `Cancel` — czyli odpowiedź na pytanie,
                   * które właśnie zostało zamknięte (2026-08-31). */
                  store.getState().dismiss();
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

            {/* ZDANIE DYSKU POD PRZYCISKIEM, KTÓRY JE WYWOŁAŁ. `AgentForm` kończy się przyciskiem
                `Save`, więc to jest wiersz bezpośrednio pod nim — i to samo miejsce obsługuje
                `Duplicate` i `Delete` niżej, bo one też są przyciskami tego panelu.

                `.enter`, nie `.fade-in`: ta powierzchnia PRZYCHODZI w miejsce, w którym przed
                kliknięciem nie było niczego, i jest odpowiedzią na gest, a nie tłem do
                przeczytania (DESIGN §7). */}
            {refusalGoes !== 'panel' ? null : (
              <p
                data-refusal
                role="alert"
                /* PASEK BŁĘDU, nie chip: `border-b` plus wypełnienie `-soft`, bez promienia
                   (DESIGN §6). Pełny obrys z wypełnieniem znaczy w tym języku pigułkę, a zdanie
                   na trzy wiersze pigułką nie jest. `-mx-4`, żeby dobiegł do krawędzi panelu —
                   `p-4` należy do panelu, a pasek jest pasmem przez całą jego szerokość. */
                className="enter -mx-4 mt-2 border-b border-fail-edge bg-fail-soft px-4 py-2 text-body text-fail"
              >
                {state.refusal}
              </p>
            )}

            {/* Kopiowanie i usuwanie dotyczą agenta, który JUŻ leży na dysku, więc dla nowego
                szkicu tych kontrolek nie ma. Przycisk, który miałby usunąć plik, którego nie ma,
                jest kontrolką bez skutku (niezmiennik 16). */}
            {draft.id === '' ? null : (
              <div className="stack mt-3 border-t border-line pt-3" data-gap="2">
                {pendingDelete === draft.id ? (
                  /* POTWIERDZENIE JEST RENDEREM, nie `window.confirm`. Dialog przeglądarki
                     blokuje webview i zabiera całą sesję — przy oknie Tauri nie ma go czym
                     odblokować. Zdanie nazywa agenta, bo „Are you sure?" nie mówi, o co pytamy,
                     a panel bywa otwarty od kilku minut. */
                  <>
                    {/* Pytanie WCHODZI: przed naciśnięciem Delete nie ma go w dokumencie
                        wcale, a staje tam, gdzie przed chwilą były dwa przyciski. Sprężyna mówi
                        „to jest nowe", zamiast pozwolić dwóm różnym rzeczom mrugnąć w jednym
                        miejscu. Drugiego regionu to zdarzenie nie rusza (ARCHITECTURE §7). */}
                    <p data-confirm-delete className="enter text-ink">
                      {deletingSays(draft.name, usage, draft.id)}
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
                        className="btn-quiet"
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
                    {/* 2026-08-31 — CZASOWNIK „EVALUATE" STOI TAM, GDZIE STOI AGENT, a nie
                        w osobnej sekcji, do której trzeba by go wpisać z pamięci. Zakłada
                        zestaw dla TEGO agenta i przechodzi do Labu: jedno kliknięcie, jedno
                        miejsce, w którym widać wynik. Przejścia nie robi ten przycisk sam —
                        robi je magazyn sekcji, ten sam, którym idzie każde inne przejście
                        (niezmiennik 13). */}
                    <button
                      data-evaluate
                      type="button"
                      /* Prymityw tej gałęzi zamiast stałej `QUIET` z trunku: ta stała zniknęła
                         razem z warstwą prymitywów (Fala 1), a przycisk przyjechał z Lab. */
                      className="btn-quiet"
                      onClick={() => {
                        void evaluateAgent(draft.id, draft.name);
                      }}
                    >
                      Evaluate
                    </button>
                    <button
                      data-duplicate
                      type="button"
                      className="btn-quiet"
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
          /* Ta sama lista, którą ten ekran właśnie wypisał, a nie własne `list_agents`:
             druga droga do tej odpowiedzi byłaby drugim miejscem, w którym mieszka „kogo
             mam zapisanych" (niezmiennik 13) — i tym, które nie widzi świeżego zapisu. */
          agents={state.agents}
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
