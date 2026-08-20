/* Kafelki rzeczy, które Loadout uruchomił dla człowieka — obok kafelków agentów, nigdy w nich.
 *
 * ZGŁOSZENIE, Z KTÓREGO TO POWSTAŁO (właściciel, 2026-08-20): „jak napiszę aby coś odpalił jakąś
 * apkę to chcę mieć też po prawej gdzie są agenci info o procesach odpalonych itp, i po kliku
 * mogę tam wejść".
 *
 * CICHA PORAŻKA, PRZED KTÓRĄ STOI TEN PLIK: kafelek, który zostaje po rzeczy, która zeszła.
 * „Running" nad komendą zeszłą dwie minuty temu jest tym samym kłamstwem, co widmowy agent
 * z T-66 i wiersz okna udający agenta z T-67 — a ta fala pokazała, że ta klasa wady wraca
 * powierzchnia po powierzchni, bo za każdym razem wchodzi przez inną powierzchnię. Kafelek
 * istnieje dokładnie tak długo, jak rzecz za nim (niezmiennik 17), i rozstrzyga to TA funkcja,
 * a nie arkusz stylów i nie komponent.
 *
 * DWIE GRUPY, NIE DWA KOLORY. Rzecz uruchomioną komendą odróżnia od agenta MIEJSCE na liście,
 * a nie odcień kwadratu: kolor jest tożsamością, nigdy stanem [DESIGN §3 „Tożsamość ≠ stan"] —
 * to ta sama reguła, przez którą w referencyjnym poprzedni prototyp agent Forge miał dokładnie ten hex,
 * który obok znaczył „czeka na twoją decyzję". Kwadrat idzie więc z tej samej przygaszonej palety
 * (`./colour.ts`), a różnicę niesie struktura odpowiedzi.
 *
 * CZEGO `railGroups` NIE ROBI: nie pyta systemu o nic. Dostaje to, co wie rejestr po stronie Rusta
 * (`src-tauri/src/commands/processes.rs`), i zamienia to na kafelki. Funkcja, która sama czytałaby
 * stan świata, nie dałaby się osądzić bez maszyny — a to jest jedyna rzecz w tej ścieżce, którą
 * da się osądzić czystym wejściem i czystym wyjściem.
 *
 * ── CO JESZCZE W TYM PLIKU MIESZKA, I DLACZEGO TUTAJ ───────────────────────────────────────
 *
 * Pod czystą funkcją stoi magazyn okna: co człowiek uruchomił, który panel ma otwarty, i jedna
 * droga od wpisanej linii do wpisu na liście. Trzy powody, dla których to jest jeden plik,
 * a nie cztery:
 *
 * (a) TO REPO NIE MA JSDOM, więc `onClick` i Enter nie odpalają się w żadnym kryterium. Stan
 *     trzymany w komponencie byłby kodem, którego nie umie dotknąć nic poza prawdziwym
 *     chromium — dokładnie ta rodzina, z której wzięło się siedemnaście kłamiących kontrolek
 *     (niezmiennik 16). Tutaj test woła to, co woła kafelek. Ten sam wybór i ten sam akapit
 *     stoją przy `../session/open.ts`.
 * (b) MAGAZYN NA POZIOMIE MODUŁU, nie w ekranie sekcji: rzecz uruchomiona komendą biegnie dalej,
 *     kiedy człowiek wejdzie do Agentów, a stan odmontowany razem z ekranem zgubiłby ją z listy,
 *     zostawiając żywy proces bez kafelka i bez Stopu.
 * (c) BLOK OWNS tego zadania wymienia ten plik i nie wymienia żadnego innego, w którym ten stan
 *     mógłby stanąć (`src/sections/run/index.tsx` nie należy do T-72). Osobny plik obok byłby
 *     ścieżką spoza zakresu, czyli pytaniem do człowieka (AGENTS.md §7), a nie cichym dopiskiem.
 *
 * SKĄD OKNO WIE, ŻE COŚ ZESZŁO. Z odświeżania: `list_processes` oddaje wszystko, co rejestr wie,
 * razem z polem `alive`, a odsiew robi `railGroups`. Nie ma tu zdarzenia z drutu i to jest wybór
 * z ceną — kafelek gaśnie do sekundy po śmierci, nie w tej samej klatce. Kanał na to jest
 * (`Channel<Vec<Line>>`), tylko wiezie WIERSZE STRUMIENIA, a rzecz uruchomiona komendą nie jest
 * agentem i nie ma w strumieniu czego pisać (niezmiennik 17).
 */
import { why } from '../../../ipc/why';
import { activeWorkspace } from '../../../state/workspaces';
import { listProcesses, startProcess, stopProcess } from '../io';
import type { RailCard } from './card';
import { railCard } from './card';

/**
 * Jedna rzecz uruchomiona komendą, tak jak widzi ją okno.
 *
 * Trzy pola, bo trzy fakty: co to jest, jak to zaadresować i czy jeszcze biegnie. Kształt jest
 * ŚWIADOMIE własny, a nie przepisany z `StartedProcess` po stronie Rusta: tam kluczem jest `pgid`,
 * czyli liczba, którą okno poznaje dopiero z odpowiedzi, a kafelek ma stanąć w chwili, w której
 * człowiek naciśnie Enter. Zlanie tych dwóch kształtów w jeden kazałoby oknu czekać z rysowaniem
 * na coś, co przyjdzie później.
 */
export interface StartedProcess {
  /** Klucz kafelka. Nigdy napis na ekranie — dokładnie jak `RailCard.id`. */
  readonly id: string;
  /**
   * Wiersz powłoki, co do znaku. To ON jest nazwą kafelka.
   *
   * Wymyślona etykieta („Dev server") byłaby relacją, której w danych nie ma (niezmiennik 17),
   * a człowiek szuka na liście tego, co sam wpisał.
   */
  readonly command: string;
  /** Czy to jeszcze biegnie. `false` znaczy „nie ma kafelka", nie „kafelek na szaro". */
  readonly alive: boolean;
}

/** Co lista dostaje: gotowe kafelki agentów i to, co wie okno o rzeczach uruchomionych. */
export interface GroupsInput {
  /** Kafelki agentów, już policzone przez `roster()`. Ten plik ich nie przelicza. */
  readonly agents: readonly RailCard[];
  /** Wszystko, o czym okno wie — także to, co już zeszło. Odsiew jest odpowiedzią tej funkcji. */
  readonly started: readonly StartedProcess[];
}

/** Dwie grupy jednej kolumny. Pusta lista znaczy „nie ma czego pokazać", nigdy „nie wiem". */
export interface RailGroups {
  readonly agents: readonly RailCard[];
  readonly started: readonly RailCard[];
}

/**
 * Kafelki obu grup — agenci tam, gdzie byli, a rzeczy uruchomione komendą obok nich.
 *
 * Rzecz, która zeszła, nie dostaje kafelka wcale. To jest cała treść tej funkcji i cały powód,
 * dla którego ona istnieje osobno od komponentu.
 */
export function railGroups(input: GroupsInput): RailGroups {
  return {
    /* KAFELKI AGENTÓW JADĄ DALEJ CO DO WARTOŚCI, nie przez `map`. Przeliczenie ich tutaj
     * postawiłoby odpowiedź na pytanie „co ten agent ostatnio powiedział" w dwóch miejscach,
     * a jedno z dwóch jest zawsze tym nieaktualnym (niezmiennik 13). Ten plik wolno DOŁOŻYĆ
     * grupę obok; przepisać tamtej nie wolno. */
    agents: input.agents,
    started: input.started.filter((one) => one.alive).map(tileFor),
  };
}

/**
 * Kafelek jednej rzeczy, która jeszcze biegnie.
 *
 * TĄ SAMĄ FUNKCJĄ, którą kafelek dostaje agent (`railCard`), i to jest wymóg, nie oszczędność:
 * kwadrat tożsamości przydziela `colour.ts` i ma go przydzielać RAZ dla całej listy. Ręczny
 * literał obok byłby drugim miejscem, w którym powstaje kafelek — a wtedy rzecz uruchomiona
 * komendą mogłaby dostać odcień z palety STANU, czyli ten sam błąd, przez który cała reguła
 * „tożsamość ≠ stan" powstała [DESIGN §3].
 */
function tileFor(one: StartedProcess): RailCard {
  return railCard({
    id: one.id,
    /* WIERSZ POWŁOKI JEST NAZWĄ, co do znaku. Etykieta wymyślona z komendy („Dev server")
     * byłaby relacją, której w danych nie ma (niezmiennik 17), a człowiek szuka na liście tego,
     * co sam wpisał. */
    name: one.command,
    /* PUSTA ROLA, bo tego faktu nie ma. „Po co ten agent jest" jest zdaniem z definicji agenta,
     * a rzecz uruchomiona komendą żadnej definicji nie ma — pusty slot kafelek po prostu
     * pomija (`rail.tsx`, `CardLine`), a zdanie zmyślone zajęłoby jego miejsce i czytałoby się
     * jak fakt. */
    role: '',
    /* ZIELONE „working", bo to znaczy „dzieje się TERAZ" [DESIGN §3] — a kafelek dostaje
     * wyłącznie rzecz, która biegnie. Rzecz, która zeszła, nie ma kafelka wcale, więc żaden
     * inny stan nie ma tu jak wystąpić. */
    status: 'working',
    /* JEDNA WYPOWIEDŹ Z PUSTYM ZDANIEM, nie pusta lista, i to jest wybór o nazwanym powodzie:
     * `sayFor([])` oddaje „Thinking…", czyli zdanie o kimś, kto MYŚLI. Nad wierszem powłoki jest
     * to relacja, której w danych nie ma (niezmiennik 17) — komenda nie myśli, komenda biegnie.
     * Puste zdanie kafelek pomija tak samo jak pustą rolę, więc zostają dwie linie, które są
     * prawdziwe: co to jest i że to się dzieje.
     *
     * Dzień, w którym ta linia zacznie nieść ostatni wiersz wyjścia, jest dniem, w którym
     * `StartedProcess` dostanie czwarte pole — a dziś nie ma, bo wyjście jedzie na drut raz
     * i tylko dla tej rzeczy, w którą człowiek wszedł (`commands::processes::Processes::said`). */
    lines: [{ kind: 'run', text: '' }],
  });
}

/* ── MAGAZYN OKNA ───────────────────────────────────────────────────────────────────────────
 *
 * Powody, dla których to stoi w tym pliku i na poziomie modułu, są w nagłówku. Poniżej jest
 * tylko to, czego `railGroups` nie umie i nie ma umieć: skąd te wpisy się biorą.
 */

/**
 * Jedna rzecz, tak jak ją TRZYMA okno: to, co widzi kafelek, plus dwa fakty, których kafelek
 * nie potrzebuje.
 *
 * Rozszerza [`StartedProcess`], a nie dokłada mu pól: tamten kształt jest wejściem czystej
 * funkcji i ma zostać najmniejszy, jaki wystarcza kafelkowi. Kafelek nie zna `pgid` z rozmysłu —
 * gdyby znał, komponent mógłby zaadresować rzecz po liczbie, którą sam sobie wyliczył.
 */
export interface Held extends StartedProcess {
  /**
   * Grupa procesów z odpowiedzi Rusta, albo `null`, dopóki nie odpowiedział.
   *
   * `null` NIE znaczy „nie biegnie": kafelek stoi w chwili naciśnięcia Enter, a odpowiedź
   * przychodzi po niej. Znaczy „nie ma jeszcze czym tego zaadresować", więc Stop się nie rysuje
   * (kontrolka bez czego ubić jest kontrolką bez handlera, niezmiennik 16), a odświeżanie tego
   * wpisu nie tyka: rzecz, o której nie wiemy, w której grupie stoi, nie jest rzeczą, o której
   * wolno powiedzieć „już jej nie ma".
   */
  readonly pgid: number | null;
  /** Ogon wyjścia — pusty, dopóki nikt w tę rzecz nie wszedł. Powód przy `StartedWire::said`. */
  readonly said: string;
}

/** Co człowiek uruchomił w tym oknie, w kolejności uruchamiania. */
let held: readonly Held[] = [];

/** Który panel jest otwarty — klucz kafelka albo `null`. Jedno pole na okno, jak `session/open`. */
let opened: string | null = null;

/**
 * Ile kluczy wybiło to okno.
 *
 * Klucz bije OKNO, nie Rust, i to jest cała treść akapitu przy [`Held::pgid`]: `pgid` przychodzi
 * po odpowiedzi, a kafelek musi mieć klucz w chwili, w której się rysuje. Rośnie, bo dwa
 * naciśnięcia Enter na tej samej komendzie to dwie rzeczy, nie jedna.
 */
let minted = 0;

const listeners = new Set<() => void>();

/** Migawka dla `useSyncExternalStore` — ta sama dla okna i dla renderu serwerowego. */
export function startedThings(): readonly Held[] {
  return held;
}

/** Klucz kafelka, którego panel jest otwarty, albo `null`. */
export function openedStarted(): string | null {
  return opened;
}

/** Powiadomienie o zmianie; oddaje funkcję, która je odwołuje. Kształt `useSyncExternalStore`. */
export function subscribeToStarted(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Otwiera panel tej rzeczy — to, co robi kliknięcie w kafelek. */
export function openStarted(id: string): void {
  if (opened === id) return;
  opened = id;
  publish();
}

/** Zamyka panel i wraca do widoku pracy. */
export function closeStarted(): void {
  if (opened === null) return;
  opened = null;
  publish();
}

function publish(): void {
  for (const listener of [...listeners]) listener();
}

/**
 * Co powiedzieć na `/start` bez komendy.
 *
 * UPRZEDZENIE, NIE DRUGA POLITYKA: tę samą odmowę ma `start_process` po stronie Rusta i to ona
 * jest zaporą (`src-tauri/src/ipc.rs`). Tutaj chodzi o to, żeby człowiek nie płacił za nią
 * wywołaniem — dokładnie ta sama para, co `whereItGoes` wobec `say_to_agent_inner`.
 */
export const NOTHING_TO_START = 'Write the command after /start, like "/start npm run dev".';

/**
 * `/start <komenda>`: stawia kafelek i uruchamia rzecz. Oddaje zdanie odmowy albo `null`.
 *
 * # Kolejność, której nie wolno odwrócić
 *
 * Wpis powstaje **przed** wywołaniem, a nie po odpowiedzi. Rzecz zamówiona komendą kończy się
 * wtedy, kiedy KOŃCZY SIĘ ONA, a nie wtedy, kiedy wraca wywołanie, które ją zamówiła — więc
 * kafelek postawiony po odpowiedzi mrugałby przy komendzie, która wstała i zaraz zeszła, a przy
 * takiej, która żyje, pojawiałby się z opóźnieniem całej granicy. Odwrotna wada — zdjęcie
 * kafelka w `finally`, tak jak `io.ts` zdejmuje pasek biegu — zgasiłaby go w tym samym tyknięciu,
 * w którym go postawiła.
 *
 * Wpis znika WYŁĄCZNIE wtedy, gdy start się nie udał: rzecz, która nie wstała, nie ma prawa mieć
 * kafelka ani jednej klatki dłużej (niezmiennik 17), a zdanie odmowy trafia w strumień, tam gdzie
 * człowiek właśnie pisał.
 *
 * ZAKRES CZYTANY W CHWILI NACIŚNIĘCIA, nie zapamiętany: człowiek mógł go przełączyć między jedną
 * linią a drugą, a komenda ma pobiec tam, gdzie stoi teraz. `null` znaczy „tam, gdzie aplikacja
 * wstała" i rozstrzyga to Rust (`AppState::project_for`) — nie odmawiamy tu za niego, bo `/start
 * ls` bez otwartego folderu jest sensowną rzeczą do zrobienia.
 */
export async function startFromLine(rest: string): Promise<string | null> {
  const command = rest.trim();
  if (command === '') return NOTHING_TO_START;

  minted += 1;
  const id = 'started-' + String(minted);
  held = [...held, { id, command, alive: true, pgid: null, said: '' }];
  publish();

  try {
    const answered = await startProcess(command, activeWorkspace()?.folder ?? null);
    /* ADRES DOPISUJEMY OSOBNO, bo między naciśnięciem Enter a tą linią mogło się stać wszystko:
     * człowiek zdążył zatrzymać coś innego, odświeżenie przepisało listę. Dlatego szukamy wpisu
     * po kluczu, a nie odtwarzamy listy z pamięci sprzed `await`. */
    held = held.map((one) =>
      one.id === id && typeof answered === 'number' ? { ...one, pgid: answered } : one,
    );
    publish();
    return null;
  } catch (error: unknown) {
    held = held.filter((one) => one.id !== id);
    publish();
    return why(error, 'Loadout could not start that.');
  }
}

/**
 * „Stop" na kafelku: kończy tę rzecz i **czeka na dowód**.
 *
 * Wpis zdejmujemy dopiero po powrocie, bo po tamtej stronie granicy `stop_process` wraca dopiero
 * z `ESRCH` dla całej grupy (niezmiennik 6). Kafelek zgaszony w chwili kliknięcia mówiłby „nie
 * żyje" nad rzeczą, która dalej pracuje i dalej pali maszynę — a to jest ta sama klasa wady, co
 * „Running" nad komendą zeszłą dwie minuty temu, tylko w drugą stronę.
 *
 * Odmowę oddajemy wołającemu zdaniem, bo jest jedna i jest prawdziwa: grupa, która po eskalacji
 * dalej odpowiada. Wpis zostaje wtedy na liście — rzecz, której nie udało się zatrzymać, jest
 * dokładnie tą, którą trzeba dalej widzieć.
 */
export async function stopStarted(id: string): Promise<string | null> {
  const one = held.find((held_) => held_.id === id);
  if (one === undefined || one.pgid === null) return null;

  try {
    await stopProcess(one.pgid);
  } catch (error: unknown) {
    return why(error, 'Loadout could not stop that.');
  }
  held = held.filter((left) => left.id !== id);
  if (opened === id) opened = null;
  publish();
  return null;
}

/**
 * Pyta rejestr, co jeszcze biegnie, i przepisuje to, co wie okno.
 *
 * To jest jedyna droga, którą okno dowiaduje się o śmierci czegoś, czego nie zatrzymało samo —
 * i dlatego jest tu odsiew po `pgid`, a nie podmiana całej listy. Trzy reguły, każda z powodem:
 *
 * 1. **Wpis bez `pgid` zostaje nietknięty.** Rust nie odpowiedział jeszcze, którą to grupa, więc
 *    jego brak w odpowiedzi nie znaczy nic. Bez tej reguły kafelek postawiony przy Enterze
 *    padałby przy pierwszym odświeżeniu, które wyprzedzi odpowiedź startu.
 * 2. **Wpis, o którym rejestr nie wie, znika.** Rejestr zapomina rzecz dopiero razem z dowodem
 *    jej śmierci, więc „nie wiem o niej" znaczy tu „już jej nie ma".
 * 3. **Rzecz, o której wie rejestr, a nie wie okno, dochodzi na listę.** Tak wraca stan po
 *    przeładowaniu okna: magazyny żyją w kontekście strony, rejestr żyje w aplikacji.
 *
 * Odpowiedź, która nie jest listą, nie zmienia NICZEGO. To jest granica atrapy z `e2e/harness.ts`
 * (odpowiada kształtem, nie stanem) i zarazem jedyna uczciwa odpowiedź na „nie dało się
 * przeczytać": cisza rejestru nie jest zdaniem „nic nie biegnie".
 */
export async function refreshStarted(): Promise<void> {
  const looking = held.find((one) => one.id === opened)?.pgid ?? null;

  let answer: unknown;
  try {
    answer = await listProcesses(looking);
  } catch {
    return;
  }
  if (!Array.isArray(answer)) return;

  const said = new Map<number, Answered>();
  for (const row of answer as readonly unknown[]) {
    const one = wireOf(row);
    if (one !== null) said.set(one.pgid, one);
  }

  const next: Held[] = [];
  for (const one of held) {
    if (one.pgid === null) {
      next.push(one);
      continue;
    }
    const fresh = said.get(one.pgid);
    if (fresh === undefined) continue;
    said.delete(one.pgid);
    /* Klucz KAFELKA zostaje ten, który okno wybiło: kwadrat tożsamości liczy się z niego
     * (`colour.ts`), więc klucz podmieniony przy odświeżeniu przemalowałby kafelek w trakcie
     * patrzenia. Wyjście bierzemy z odpowiedzi tylko wtedy, gdy o nie pytaliśmy — dla pozostałych
     * rzeczy jedzie `null` i zastąpienie nim tego, co już mamy, wygasiłoby otwarty panel. */
    next.push({
      ...one,
      command: fresh.command,
      alive: fresh.alive,
      said: fresh.said === '' ? one.said : fresh.said,
    });
  }
  for (const one of said.values()) {
    minted += 1;
    next.push({ ...one, id: 'started-' + String(minted) });
  }

  held = next;
  publish();
}

/**
 * Wiersz, który przyszedł z rejestru: to samo, co [`Held`], tylko `pgid` jest tu znane z definicji.
 *
 * Osobny typ, a nie drugi warunek przy każdym użyciu: `null` w tamtym polu znaczy „okno nie
 * dostało jeszcze odpowiedzi", a wiersz, który właśnie z odpowiedzi przyjechał, tego stanu mieć
 * nie może. Bez tego zawężenia każde wzięcie `pgid` z odpowiedzi wymagałoby gałęzi, której nie
 * da się wykonać — a gałąź nieosiągalna czyta się jak przypadek, który ktoś przewidział.
 */
type Answered = Held & { readonly pgid: number };

/**
 * Jeden wiersz odpowiedzi Rusta, sprawdzony polami — albo `null`.
 *
 * Sprawdzamy POLA, nie klasę: po drugiej stronie granicy nie ma żadnych klas, jest JSON, a wiersz
 * o innym kształcie niż `StartedWire` znaczy, że rozjechały się dwie strony szwu. Wiersz
 * odrzucony po cichu jest lepszy niż `undefined` wjeżdżające na kafelek jako nazwa — ten sam
 * wybór, co przy `saidBy` w `../../../ipc/why.ts`.
 */
function wireOf(row: unknown): Answered | null {
  if (typeof row !== 'object' || row === null) return null;
  const said = row as { pgid?: unknown; command?: unknown; alive?: unknown; said?: unknown };
  if (typeof said.pgid !== 'number' || typeof said.command !== 'string') return null;
  return {
    /* Klucz zastępczy dla wiersza, którego okno nie znało; wołający nadaje mu swój, jeśli go
     * dokłada do listy. Z `pgid`, bo to jedyna rzecz, którą ten wiersz o sobie mówi na pewno. */
    id: 'started-pgid-' + String(said.pgid),
    command: said.command,
    alive: said.alive === true,
    pgid: said.pgid,
    said: typeof said.said === 'string' ? said.said : '',
  };
}
