/* Ekran sekcji Agents: nagłówek, spis ról po lewej i CAŁA otwarta rola po prawej. Pusta
 * biblioteka dalej jest zaproszeniem z makiety (`docs/mockup/index.html`, `data-screen="agents"`).
 *
 * CO SIĘ ZMIENIŁO 2026-08-31 WIECZOREM I DLACZEGO — drugie zgłoszenie właściciela, dwa zrzuty
 * i jedno zdanie: „a i to powinno byc domyslnie, wyjeb ten widok tu".
 *
 * Do tego wieczora ekran wstawał jako ŚCIANA KAFELKÓW na całą szerokość, kafelek na agenta,
 * cztery wiersze każdy. Układ, który naprawdę daje się czytać — spis nazw po lewej i cała rola
 * po prawej — istniał od rana i stał ZA KLIKNIĘCIEM: montował się dopiero, gdy człowiek trafił
 * w kafelek. Arytmetyka jego własnej biblioteki przewróciła tamten widok:
 *
 *   - dwadzieścia dziewięć ról razy cztery wiersze to kilometry przewijania, a w oknie mieści
 *     się sześć pozycji;
 *   - kafelek niósł PIERWSZE 150 ZNAKÓW instrukcji, czyli mniej więcej jedno zdanie z dwudziestu.
 *     Żeby dowiedzieć się, czym rola jest, dalej trzeba było ją otworzyć — po kolei, dwadzieścia
 *     dziewięć razy;
 *   - a te same 150 znaków było na kafelku razem z pięcioma faktami o modelu, z których każdy
 *     jest POLEM FORMULARZA stojącego obok (niezmiennik 13).
 *
 * TRZY ROZSTRZYGNIĘCIA, KTÓRYCH ZLECENIE NIE PODAŁO, i ich powody:
 *
 *   1. PRAWA KOLUMNA NIGDY NIE JEST PUSTĄ DZIURĄ, bo zawsze stoi w niej rola: przy wejściu
 *      pierwsza z listy (`standing` niżej). „Nic nie wybrano" nie jest stanem, który ten ekran
 *      umie pokazać — a to jest jedyna wersja, w której nie trzeba wymyślać treści zastępczej.
 *      Wybór pierwszej pozycji jest przy tym jedynym wyborem, którego nie trzeba tłumaczyć:
 *      jest powtarzalny, nie zależy od historii i pokrywa się z tym, na czym stoi oko.
 *   2. `＋ Create` PRZEPROWADZA SIĘ DO SPISU, na miejsce, w którym stało `All agents`. Tamten
 *      przycisk wracał DO ŚCIANY KAFELKÓW, czyli do widoku, którego już nie ma — kontrolka bez
 *      celu jest kontrolką bez skutku (niezmiennik 16). Jest cicha, nie akcentowana, bo przy
 *      otwartej roli jedyną czynnością główną tego ekranu jest `Save` (DESIGN §6).
 *   3. `Cancel` STOI TYLKO WTEDY, GDY JEST CO ANULOWAĆ. Arkusz czyta rolę wprost z magazynu,
 *      dopóki człowiek czegoś w niej nie zmieni; dopiero wtedy powstaje szkic i pojawia się
 *      przycisk, który ma go czym cofnąć. `Cancel` nad niezmienioną rolą byłby przyciskiem,
 *      po którym nie dzieje się nic.
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
import type { Agent, AgentsIo, Color } from '../../state/agents';
import { createAgentsStore } from '../../state/agents';
import { problemSays } from '../../state/library';
import { askedAgent, subscribeToAsked, takeAskedAgent } from '../../ui/palette/asked';
import { evaluateAgent } from '../lab/evaluate';
import { ImportSetup } from '../import';
import { AgentForm } from './agent-form';
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

/* PIĘĆ BRZMIEŃ O MODELU ZESZŁO Z TEGO PLIKU 2026-08-31 WIECZOREM, i to jest usunięcie drugiej
 * kopii, nie usunięcie treści. Stały tu tabele `FILE_ACCESS_SAYS` i `THINKING_SAYS` oraz
 * `VENDOR_SAYS` czytane z `VENDORS`, wyłącznie po to, żeby złożyć wiersz
 * „Claude Code · opus · Balanced · Work freely · gives up after 20m" na kafelku listy. Każdy
 * z tych pięciu faktów jest POLEM formularza, który od tej zmiany stoi w tym samym kadrze,
 * o dwadzieścia pikseli obok — więc wiersz mówił po raz drugi to, co czyta się w kontrolce
 * (niezmiennik 13), i to głośniej niż nazwę, którą napisał człowiek. Brzmienia z tabeli
 * „We say / We never say" [T4 §8.1] mieszkają dziś w jednym miejscu: w `agent-form.tsx`. */

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

/* CZTERY FUNKCJE ZESZŁY STĄD 2026-08-31 WIECZOREM RAZEM ZE ŚCIANĄ KAFELKÓW, i każda dlatego,
 * że była odpowiedzią na pytanie, którego kafelek już nie zadaje:
 *
 *   `giveUpSays` i `factsSay`  składały wiersz pięciu faktów o modelu. Wszystkie pięć są dziś
 *                              polami formularza stojącego w tym samym kadrze — powód przy
 *                              tabelach brzmień wyżej.
 *   `WORDS` i `roleWords`      ucinały instrukcje do 150 znaków, bo tyle mieściło się w jednej
 *                              linii kafelka. Prawa kolumna pokazuje je W CAŁOŚCI, w polu,
 *                              które można podnieść przyciskiem `Taller` — więc przycinanie
 *                              w danych przestało być oszczędnością i zostało samą stratą.
 *   `cascade`                  rozkładała wejście osiemnastu kafelków po 24 ms. Spis nazw wchodzi
 *                              razem z ekranem i nic w nim nie „przybywa" po kolei; kaskada nad
 *                              wierszem, na który nikt nie czeka, jest wyłącznie czekaniem.
 *
 * `usageSays` i `usedIn` ZOSTAJĄ i mają dziś dwóch wołających w tym pliku: wiersz `used in …`
 * w nagłówku otwartej roli i pytanie przed usunięciem, dwa wiersze pod nim. To jest ta sama
 * liczba w dwóch zdaniach o TEJ SAMEJ roli, a nie dwa źródła prawdy — druga strona tego faktu
 * stoi przy `deletingSays` wyżej. */

export default function AgentsScreen({
  store = OWN_STORE,
  usage: usageProp,
  opened,
  confirming,
}: AgentsScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  /* KTÓRĄ ROLĘ TRZYMA PRAWA KOLUMNA — sam identyfikator, nie kopia agenta.
   *
   * `null` znaczy „człowiek jeszcze żadnej nie wskazał" i wtedy stoi pierwsza z listy (patrz
   * `standing` niżej). Pusty napis znaczy NOWĄ rolę, dokładnie tak samo jak puste `id` znaczy
   * ją dla `store.save` — jedna umowa, nie druga.
   *
   * Identyfikator, a nie obiekt: rola wskazana ręką ma się przerysowywać, kiedy magazyn ją
   * zmieni (zapis, kopia, klik w kwadrat tożsamości). Kopia zamrożona w stanie ekranu
   * pokazywałaby wersję sprzed tamtej zmiany i byłaby drugim zdaniem o tym, co leży na dysku
   * (niezmiennik 4). */
  const [picked, setPicked] = useState<string | null>(opened === undefined ? null : opened.id);
  /* Co człowiek w tej roli ZMIENIŁ i jeszcze nie zapisał. `null` znaczy „nic" — i to jest
   * różnica, która decyduje o tym, czy `Cancel` w ogóle stoi na ekranie. */
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

  /**
   * KTÓRA ROLA STOI W CIELE EKRANU — jedna odpowiedź, trzy źródła, policzona w jednym miejscu.
   *
   * Kolejność nie jest gustem, tylko listą pytań od najmocniejszego do najsłabszego:
   *
   *   1. szkic         — człowiek coś w niej zmienił, więc widzi swoje litery, nie dysk;
   *   2. wskazana ręką — kliknął ten wiersz spisu, albo zapisał ją i `save` wpisało tu
   *      identyfikator, który przyjął dysk (dla NOWEJ roli wybija go mennica po stronie Rusta
   *      i ekran nie zna go inaczej niż przez `justSaved` — powód w `src/state/agents.ts`);
   *   3. pierwsza z listy — wejście na sekcję. To jest to rozstrzygnięcie, którego zlecenie nie
   *      podało: prawa kolumna nigdy nie jest pustą dziurą, bo zawsze stoi w niej rola.
   *
   * `justSaved` NIE JEST W TYM ŁAŃCUCHU, i to jest wybór: magazyn kasuje to pole na wejściu do
   * KAŻDEJ czynności, więc klik w kwadrat tożsamości innego wiersza przerzucałby arkusz na tamtą
   * rolę w połowie zapisu. Wskazanie przepisujemy stamtąd RAZ, w chwili udanego zapisu, i od tej
   * chwili trzyma je ten stan.
   *
   * `structuredClone`, a nie wiersz magazynu wprost: formularz buduje następną wartość przez
   * `{ ...value, pole }`, więc bez kopii `skills`, `connections` i `vendorOptions` byłyby TYMI
   * SAMYMI tablicami, co w magazynie — a wtedy pierwsza zmiana listy w arkuszu przepisywałaby
   * po cichu agenta na dysku. Ta sama pułapka, którą opisuje `duplicate` w `src/state/agents.ts`.
   */
  const fromLibrary = (id: string | null): Agent | undefined =>
    id === null || id === '' ? undefined : state.agents.find((agent) => agent.id === id);

  const chosen = fromLibrary(picked) ?? state.agents[0];
  const standing: Agent | null = draft ?? (chosen === undefined ? null : structuredClone(chosen));

  /* Jedna funkcja na całą sekcję i to jest cały sens niezmiennika 16: przycisk w spisie
   * i przycisk w zaproszeniu są dwoma wejściami do JEDNEJ ścieżki. */
  const startDraft = (): void => {
    /* Drugie kliknięcie w `＋ Create` NIE kasuje tego, co człowiek zdążył wpisać w nowej roli.
     * Kliknięcie przy otwartej roli ZAPISANEJ zaczyna nową — bo o to właśnie prosi, a szkic
     * tamtej roli nie ma prawa wjechać pod nagłówek „New agent". */
    setDraft((open) => (open !== null && open.id === '' ? open : blankAgent(state.agents.length)));
    setPicked('');
    setExpanded(false);
    setPendingDelete(null);
    /* Odmowa sprzed chwili dotyczyła CZEGOŚ INNEGO niż to, co człowiek właśnie otwiera —
     * a od 2026-08-31 zdanie przy otwartej roli stoi pod jej przyciskiem `Save`, więc
     * zostawione tam czytałoby się jak odpowiedź na kliknięcie, którego jeszcze nie było. */
    store.getState().dismiss();
  };

  /* Otwiera ZAPISANEGO agenta: zapamiętuje, który to, i porzuca szkic poprzedniego. Kopii tu
   * nie robimy, bo arkusz czyta rolę z magazynu, dopóki człowiek czegoś w niej nie zmieni —
   * kopia powstaje w `standing` wyżej i to ona jedzie do formularza. */
  const open = (agent: Agent): void => {
    setPicked(agent.id);
    setDraft(null);
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

  /* KLIK W KWADRAT W SPISIE ZMIENIA TOKEN TOŻSAMOŚCI — 2026-08-31.
   *
   * `Colour` wypadł z formularza, bo nie wymagał ani jednej decyzji i stał nad `Instructions`.
   * To jest miejsce, do którego ta decyzja poszła: nie znika, tylko przestaje pytać. Jedzie
   * TĄ SAMĄ krawędzią do dysku, co Save (`store.save`), więc odmowa ma gdzie wylądować
   * i ląduje (`refusalGoes` niżej).
   *
   * Kopia przez `structuredClone`: obiekt z listy niesie `skills`, `connections`
   * i `vendorOptions`, a mutowanie wiersza magazynu w miejscu pokazuje na ekranie zmianę,
   * której nikt nie zapisał.
   *
   * SZKIC TEJ SAMEJ ROLI DOSTAJE BARWĘ RAZEM Z DYSKIEM — 2026-08-31 wieczorem. Do tej zmiany
   * kwadraty stały na kafelkach, a kafelki znikały pod otwartym panelem, więc te dwa miejsca
   * nie mogły być na ekranie naraz. Dziś mogą: spis stoi obok arkusza przez cały czas. Bez tej
   * linii klik w kwadrat otwartej roli zmieniałby barwę na dysku, a najbliższy `Save` odsyłałby
   * tam barwę sprzed kliknięcia — czyli cofał czynność, o której nikt nie powiedział, że się
   * nie udała. */
  const repaint = (agent: Agent): void => {
    const colour = nextIdentity(agent.color);
    const next = structuredClone(agent);
    next.color = colour;
    setDraft((open) => (open !== null && open.id === agent.id ? { ...open, color: colour } : open));
    void store.getState().save(next);
  };

  const save = (agent: Agent): void => {
    /* SZKIC ZNIKA TYLKO WTEDY, GDY DYSK POTWIERDZIŁ — a rola zostaje w kadrze. Nieudany zapis
     * zostawia szkic z tym, co człowiek wpisał, a zdanie odmowy stoi w magazynie i jest
     * wyrenderowane niżej (niezmiennik 4: ekran zgadza się z tym, co naprawdę leży na dysku).
     *
     * Udany zapis kasuje SZKIC i przepisuje WSKAZANIE z `justSaved`, i to nie jest zamknięcie
     * arkusza: rola zostaje w kadrze. Dla nowej roli identyfikator wybija mennica po stronie
     * Rusta i to pole jest jedynym miejscem, z którego ekran może go poznać — bez tej linii
     * udany zapis nowej roli odsyłałby na pierwszą z listy. */
    void store
      .getState()
      .save(agent)
      .then((saved) => {
        if (!saved) return;
        setDraft(null);
        setPicked(store.getState().justSaved);
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
   * kontrolki, które przy otwartej roli mogą dostać odmowę — Save, Duplicate, Delete — są
   * przyciskami W TYM ARKUSZU. Kiedy żadna rola nie stoi, jedyną czynnością jest odczyt
   * biblioteki, więc zdanie wraca pod nagłówek.
   *
   * WARUNEK ZMIENIŁ SIĘ Z `draft` NA `standing` 2026-08-31 wieczorem, i to jest ta sama reguła
   * powiedziana o nowym układzie: pytanie brzmi „czy na ekranie stoi rola z przyciskami", a nie
   * „czy człowiek już coś w niej zmienił". Zostawione na `draft` odesłałoby odmowę odczytu barwy
   * z kwadratu w spisie pod nagłówek, dwie kolumny od miejsca, w które człowiek patrzy.
   *
   * `body` bije arkusz, bo katalog, którego nie da się przeczytać, jest największym faktem na
   * tym ekranie — a wtedy nie stoi w nim żadna rola, więc te dwa wyjścia i tak się nie spotykają. */
  const refusalGoes: 'nowhere' | 'body' | 'panel' | 'bar' =
    state.refusal === null
      ? 'nowhere'
      : standing !== null
        ? 'panel'
        : shows === 'unreadable'
          ? 'body'
          : 'bar';

  return (
    <section className="flex h-full flex-col">
      {/* `.screen-head` niesie wysokość 52 px, odstępy i kreskę pod spodem; tła nie niesie
          z rozmysłu, więc dokłada je `.glass`. Reguła jest jedna: szkło jest chrome, papier
          jest treścią — pasek nagłówka jest chrome i nic pod nim się nie czyta. */}
      <header className="screen-head glass">
        <h1 className="text-title text-ink">Agents</h1>

        {/* NAGŁÓWEK ZOSTAJE PRZY BIBLIOTECE, A ＋ CREATE SCHODZI DO SPISU — 2026-08-31 wieczorem.
         *
         * Do tego wieczora te kontrolki znikały przy otwartym panelu, bo DESIGN §6 daje ekranowi
         * dokładnie JEDNĄ czynność główną, a `＋ Create` w akcencie tutaj i `Save` w akcencie
         * tam były dwiema. Od kiedy rola stoi w ciele ekranu ZAWSZE, ta sama reguła znaczy coś
         * innego: znikanie zabrałoby obie kontrolki na dobre, czyli sekcja straciłaby jedyną
         * drogę do zrobienia nowego agenta. `＋ Create` przeprowadza się więc na górę spisu,
         * do miejsca po `All agents`, i jest CICHY — jedyną czynnością główną zostaje `Save`.
         *
         * `Import setup` zostaje w nagłówku i stoi zawsze: to jest czynność BIBLIOTEKI, a nie
         * tej jednej roli, i jest drogą wejścia dla człowieka, który nie ma jeszcze ani jednego
         * agenta. Licznik żyje tylko wtedy, gdy jest co liczyć — `0 saved` obok
         * `No agents yet.` to ten sam fakt w dwóch miejscach (niezmiennik 13). */}
        <button
          type="button"
          className="btn-quiet ml-auto"
          onClick={() => {
            setImporting(true);
          }}
        >
          Import setup
        </button>

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
        {/* SPIS RÓL PO LEWEJ, CAŁA ROLA PO PRAWEJ — I TAK EKRAN SIĘ OTWIERA, bez klikania.
            2026-08-31 wieczorem, zlecenie właściciela.

            Do tego wieczora ten układ istniał, ale stał ZA kliknięciem: domyślnie ekran wstawał
            jako ściana kafelków na całą szerokość. Zmierzone na bibliotece właściciela: 29 ról
            po cztery wiersze każda, sześć pozycji w oknie, 150 znaków promptu na kafelek. Żeby
            dowiedzieć się, czym rola jest, trzeba ją było otworzyć — dwadzieścia dziewięć razy.

            Dziś rola bierze ciało ekranu od razu, a biblioteka jest spisem nazw po lewej. Spis
            nie jest ozdobą: to jest CAŁA biblioteka, przełączanie kosztuje jedno kliknięcie,
            a nazwa, na której stoisz, jest widoczna razem z pozostałymi.

            SPIS ZNIKA, KIEDY NIE MA CZEGO SPISYWAĆ. Pierwsza rola powstaje na pustej bibliotece
            (`＋ Create` w zaproszeniu), a kolumna z niczym w środku jest miejscem zabranym treści.

            `flex-none` BIJE `flex: 1 1 auto` Z PRYMITYWU, i to jest cała różnica między spisem
            a drugą pustką: `.screen-body` rośnie z definicji, więc sam `w-64` zostawiał kolumnę
            z sześcioma nazwami rozciągniętą na 735 px obok arkusza, który miał się rozciągnąć.
            Zmierzone na zrzucie, nie wydedukowane. */}
        {shows !== 'library' ? null : (
          <div data-agent-index className="screen-body w-64 flex-none border-r border-line">
            {/* JEDYNA DROGA DO NOWEJ ROLI, i stoi tam, gdzie stało `All agents`. Tamten przycisk
                wracał DO ŚCIANY KAFELKÓW — do widoku, którego już nie ma — więc zniknął razem
                z nią, a nie zamiast niego. Cichy, bo czynnością główną tego ekranu jest `Save`
                w arkuszu obok (DESIGN §6: dokładnie jedna). */}
            <button
              data-create
              type="button"
              className="btn-quiet mb-3 w-full"
              onClick={startDraft}
            >
              ＋ Create
            </button>
            <ul className="stack" data-gap="1">
              {state.agents.map((agent) => (
                /* `relative`, bo kwadrat tożsamości stoi OBOK przycisku otwierającego, a nie
                   w nim: przycisk w przycisku nie jest poprawnym dokumentem, przeglądarka
                   rozrywa go przy budowaniu drzewa i wewnętrzny przestaje odpowiadać na
                   kliknięcia. Nakładka trzyma oba w tym samym wierszu i zostawia CAŁY wiersz
                   klikalny. */
                <li key={agent.id} className="relative">
                  {/* WIERSZ JEST PRZYCISKIEM, i to on niesie `data-agent`: od tej zmiany to jest
                      kontrolka, którą człowiek otwiera rolę. Do 2026-08-18 kafelek był `<li>`
                      bez handlera i zapisany agent zostawał na liście na zawsze, z każdą
                      literówką w instrukcjach.

                      `aria-current`, bo `.row` maluje z niego pozycję bieżącą (theme.css) —
                      spis, który nie mówi, którą rolę trzymasz otwartą, każe jej szukać wzrokiem
                      po każdym kliknięciu.

                      `pl-9` robi miejsce kwadratowi, który leży NA wierszu, a nie w jego treści:
                      8 px do lewej krawędzi kwadratu plus 22 px kwadratu. */}
                  <button
                    data-agent={agent.id}
                    data-just-saved={state.justSaved === agent.id ? '' : undefined}
                    type="button"
                    aria-current={
                      standing !== null && agent.id === standing.id ? 'true' : undefined
                    }
                    className="row w-full pl-9"
                    onClick={() => {
                      open(agent);
                    }}
                  >
                    <span className="min-w-0 truncate">{agent.name}</span>
                    {/* CO WŁAŚNIE ZASZŁO — 2026-08-31, zgłoszenie właściciela. Do tego dnia udany
                        `Save` dawał dokładnie ten sam widok, co `Cancel`, a `Duplicate` nie
                        zmieniał ani jednego widocznego piksela. Ta plakietka jest jedyną
                        różnicą i wchodzi SPRĘŻYNĄ, bo pojawia się nad tym, co już stoi na
                        ekranie (DESIGN §7).

                        SŁOWO, nie sam obrys: „Saved" mówi, CO się stało, a barwa mówi tylko
                        „coś tu". Akcent, nie kolor stanu — to nie jest ani „teraz", ani
                        „zepsute", tylko wskazanie miejsca, w którym zaszła zmiana (DESIGN §3).
                        Znika przy następnej czynności; kasuje ją magazyn na wejściu do każdej
                        z nich (`justSaved` w `src/state/agents.ts`). */}
                    {state.justSaved === agent.id ? (
                      <span className="chip enter ml-auto shrink-0" data-tone="accent">
                        Saved
                      </span>
                    ) : null}
                  </button>
                  {/* KWADRAT JEST KONTROLKĄ, i jest jedynym miejscem, w którym barwa tożsamości
                      da się jeszcze zmienić — `Colour` wypadł z formularza (`agent-form.tsx`).
                      Nazwa mówiona na głos, bo cała treść tego przycisku to jedna litera.

                      `inset-y-0 my-auto` zamiast liczby od góry: wysokość wiersza bierze się
                      z drabinki stopni, a nie z żadnej stałej w tym pliku, więc kwadrat, który
                      ma zostać wyśrodkowany po zmianie stopnia, nie może mieć wpisanego odstępu. */}
                  <button
                    data-identity={agent.color}
                    type="button"
                    aria-label={`Change the colour of ${agent.name}`}
                    className={`${SQID} ${ID_COLOUR[agent.color]} absolute inset-y-0 left-2 my-auto`}
                    onClick={() => {
                      repaint(agent);
                    }}
                  >
                    {initial(agent.name)}
                  </button>
                </li>
              ))}
              {/* WADLIWY PLIK ZOSTAJE W SPISIE i nie jest kontrolką: nie da się go otworzyć
                  w arkuszu obok, bo nie dał się przeczytać. Zdanie mówi, co z nim zrobić. */}
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
          </div>
        )}

        {/* CIAŁO EKRANU: rola, albo jedno z czterech zdań o tym, dlaczego roli nie ma.
            `standing` bije wszystkie cztery, bo pierwsza rola powstaje na PUSTEJ bibliotece —
            i wtedy zaproszenie ustępuje jej miejsca, zamiast stać obok niej. */}
        {standing !== null ? null : (
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
              /* PUSTA BIBLIOTEKA ZOSTAJE DOKŁADNIE TAKA, JAKA BYŁA, i to jest rozstrzygnięcie
                 właściciela z tego samego zlecenia: tego stanu nie tykamy. */
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
              /* SAME WADLIWE PLIKI, ANI JEDNEJ ROLI DO OTWARCIA. Bez tej gałęzi prawa kolumna
                 byłaby pustym prostokątem obok spisu, w którym każdy wiersz mówi, że się nie
                 udał — czyli ekranem, który wygląda na zepsuty dwa razy. Zaproszenia tu nie ma:
                 `＋ Create` stoi na górze spisu i jest jedno na cały ekran (niezmiennik 13). */
              <div className="flex h-full flex-col items-center justify-center gap-3 px-4 text-center">
                <span className="mark">◇</span>
                <p className="text-ink">Nothing here can be opened yet.</p>
                <p className="lead">
                  Every file beside this one needs a hand before Loadout can read it. Put them
                  right, or write a new agent.
                </p>
              </div>
            )}
          </div>
        )}

        {standing === null ? null : (
          /* ARKUSZ WCHODZI Z EKRANEM, NIE NAD NIM — 2026-08-31 wieczorem. Do tego wieczora
             powierzchnia wjeżdżała sprężyną (`.enter`), bo pojawiała się nad listą, której
             przed kliknięciem nie zasłaniała. Dziś stoi tam od pierwszej klatki, więc nie
             PRZYCHODZI i nic tu nie ma prawa skakać: zostaje samo `opacity` (DESIGN §7).

             `key` po roli, którą trzyma, i to nie jest optymalizacja: bez niego przełączenie na
             inną rolę zostawiłoby otwarte `Taller`, `Runs with` i `Advanced` z poprzedniej —
             czyli formularz, który pamięta cudze rozwinięcia. Przy okazji `.fade-in` odgrywa
             się przy każdym przełączeniu i mówi oku, że treść pod spodem jest już inna.

             `.glass` zamiast `bg-panel`: arkusz jest chrome, a nie kartką z treścią. Obrys lewej
             krawędzi zostaje klejem układu — arkusz przylega do krawędzi okna, więc `.pane`
             z obrysem dookoła i promieniem rysowałby ramkę wiszącą w powietrzu.

             KLAMRY WOKÓŁ TEGO KOMENTARZA BYŁYBY BŁĘDEM SKŁADNI, 2026-08-31. Komentarz owinięty
             w klamry jest komentarzem JSX i działa wyłącznie tam, gdzie stoją DZIECI elementu.
             Tutaj jesteśmy już wewnątrz wyrażenia (po `? null : (`), więc klamra otwierałaby
             drugie wyrażenie i esbuild mówi `Expected ")" but found "className"`. Ta wersja,
             bez klamer, jest zwykłym komentarzem JS i jest poprawna w obu kontekstach. */
          <aside
            key={standing.id === '' ? 'new-agent' : standing.id}
            data-role-sheet
            /* SZEROKOŚĆ BIERZE Z CIAŁA EKRANU, NIE ZE STAŁEJ. Do 2026-08-31 stało tu `w-83`,
               czyli 332 px, i to była cała wina za wrażenie „wąska rura obok szerokiej pustki":
               dziewięć wierszy w 332 px jest wyższe niż okno, więc `Save` stał pod krawędzią,
               a pole instrukcji — jedyna rzecz, która JEST rolą — dostawało w tej rurze 150 px
               przy 546 px niewykorzystanej wysokości obok.

               `flex flex-col` plus `min-h-0`, bo to jest połowa, która wpuszcza wysokość do
               środka: formularz niżej rozciąga swój wiersz instrukcji dokładnie o tyle, ile tu
               zostanie. Bez kolumny elastycznej `flex-1` w formularzu nie miałoby czego dzielić. */
            className="fade-in glass flex min-h-0 flex-1 flex-col overflow-auto border-l border-line p-4"
          >
            {/* Nagłówek arkusza trzyma SIĘ TEJ SAMEJ kolumny, co pola pod nim: `Cancel` odbity
                do krawędzi powierzchni stałby 240 px za ostatnim polem i nie należałby wzrokiem
                do niczego (2026-08-31, zmierzone na zrzucie). */}
            <div className="flex w-full max-w-192 shrink-0 items-baseline gap-3 pb-3">
              {/* NAZWA NIE USTĘPUJE NIKOMU. Do 2026-08-31 to metadana miała `shrink-0`, więc
                  ucinana była nazwa, żeby zmieścił się model, którego nikt nie wybierał. Treść
                  pisana przez człowieka ustępuje tylko treści pisanej przez człowieka. */}
              <h2 className="shrink-0 text-heading text-ink">
                {standing.id === '' ? 'New agent' : standing.name}
              </h2>

              {/* ILE WORKFLOW STRACI TĘ ROLĘ — jeden fakt, jedno miejsce, i to jest miejsce nad
                  przyciskiem, który go potrzebuje: pytanie przed `Delete` niżej pyta TĄ SAMĄ
                  liczbą (`deletingSays`). Do 2026-08-31 wieczorem wiersz stał na kafelku listy,
                  czyli o dwie kolumny od pytania, które z niego korzysta.

                  `null` w `usage` znaczy „katalogu workflow NIE UDAŁO SIĘ przeczytać" i wtedy
                  tego wiersza nie ma wcale — zero wypisane z nieodbytego odczytu jest zdaniem
                  nieprawdziwym, a nie ostrożnym (niezmiennik 17). Nowa rola nie ma jeszcze
                  identyfikatora, więc nie ma jej też kto liczyć.

                  `min-w-0 truncate` bez `shrink-0`: to metadana skraca się pierwsza. */}
              {usage === null || standing.id === '' ? null : (
                <span data-facts className="min-w-0 truncate font-mono text-meta text-muted">
                  {usageSays(usedIn(usage, standing.id))}
                </span>
              )}

              {/* `Cancel` STOI TYLKO WTEDY, GDY JEST CO ANULOWAĆ — 2026-08-31 wieczorem.
                  Arkusz czyta rolę wprost z magazynu, dopóki człowiek czegoś w niej nie zmieni,
                  więc do pierwszej litery nie ma szkicu, który dałoby się cofnąć: przycisk
                  stojący tu wcześniej byłby kontrolką bez skutku (niezmiennik 16). Do tej
                  zmiany znaczył „zamknij panel i wróć do kafelków" — a kafelków już nie ma. */}
              {draft === null ? null : (
                <button
                  type="button"
                  className="btn-quiet ml-auto"
                  onClick={() => {
                    /* Porzucenie szkicu ZAPISANEJ roli zostawia ją na ekranie i przywraca jej
                     * wersję z dysku; porzucenie NOWEJ zdejmuje też wskazanie, bo pusty
                     * identyfikator nie wskazuje na nic, co dałoby się otworzyć. */
                    if (standing.id === '') setPicked(null);
                    setDraft(null);
                    setPendingDelete(null);
                    /* Porzucenie edycji jest porzuceniem też zdania o niej. Zostawione,
                     * wskoczyłoby pod nagłówek sekcji (`refusalGoes`) już po tym, jak człowiek
                     * odpowiedział na nie `Cancel` — czyli odpowiedź na pytanie, które właśnie
                     * zostało zamknięte (2026-08-31). */
                    store.getState().dismiss();
                  }}
                >
                  Cancel
                </button>
              )}
            </div>

            {/* KOLUMNA CZYTANIA WEWNĄTRZ SZEROKIEJ POWIERZCHNI — 2026-08-31.
                Arkusz bierze całą pozostałą szerokość ciała ekranu, ale formularz jest listą
                wierszy, a jednowierszowe pole `Name` na tysiąc pikseli nie jest ani czytelne,
                ani szybsze do wypełnienia. Szerokość dostaje więc TREŚĆ arkusza, a nie każde
                pole z osobna: 768 px to ta sama miara, którą ma kolumna czytania w Knowledge.
                `flex-1 min-h-0` przewleka wysokość dalej, do wiersza instrukcji. */}
            <div className="flex min-h-0 w-full max-w-192 flex-1 flex-col">
              <AgentForm
                value={standing}
                expanded={expanded}
                onChange={setDraft}
                onToggleMore={() => {
                  setExpanded((wasOpen) => !wasOpen);
                }}
                onSave={() => {
                  save(standing);
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
                   na trzy wiersze pigułką nie jest. Bez ujemnego marginesu: ten
                   pasek stoi w kolumnie czytania, bo od 2026-08-31 arkusz jest szerszy niż zdanie:
                   pasmo na tysiąc pikseli pod dwuwyrazową odmową czyta się jak awaria okna. */
                  className="enter mt-2 shrink-0 border-b border-fail-edge bg-fail-soft px-3 py-2 text-body text-fail"
                >
                  {state.refusal}
                </p>
              )}

              {/* Kopiowanie i usuwanie dotyczą agenta, który JUŻ leży na dysku, więc dla nowego
                szkicu tych kontrolek nie ma. Przycisk, który miałby usunąć plik, którego nie ma,
                jest kontrolką bez skutku (niezmiennik 16). */}
              {standing.id === '' ? null : (
                <div className="stack mt-3 shrink-0 border-t border-line pt-3" data-gap="2">
                  {pendingDelete === standing.id ? (
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
                        {deletingSays(standing.name, usage, standing.id)}
                      </p>
                      <div className="flex items-center gap-2">
                        <button
                          data-delete-confirm
                          type="button"
                          className={DANGER}
                          onClick={() => {
                            const doomed = standing.id;
                            setPendingDelete(null);
                            setDraft(null);
                            /* Wskazanie schodzi razem z plikiem: zostawione, wskazywałoby na
                             * rolę, której już nie ma, i `standing` odesłałby na pierwszą
                             * z listy dopiero przez `undefined`. Mówimy to wprost. */
                            setPicked(null);
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
                          void evaluateAgent(standing.id, standing.name);
                        }}
                      >
                        Evaluate
                      </button>
                      <button
                        data-duplicate
                        type="button"
                        className="btn-quiet"
                        onClick={() => {
                          /* Kopia otwiera się w arkuszu, bo pierwsze, co się z nią robi, to
                           * nadaje jej własną nazwę. Identyfikator wybiła mennica, więc ekran
                           * zna go tylko przez `justSaved` — i tylko wtedy, gdy zapis doszedł:
                           * po odmowie wskazanie zostaje tam, gdzie było. */
                          void store
                            .getState()
                            .duplicate(standing.id)
                            .then(() => {
                              const minted = store.getState().justSaved;
                              if (minted !== null) setPicked(minted);
                            });
                        }}
                      >
                        Duplicate
                      </button>
                      <button
                        data-delete
                        type="button"
                        className={`ml-auto ${DANGER}`}
                        onClick={() => {
                          setPendingDelete(standing.id);
                        }}
                      >
                        Delete
                      </button>
                    </div>
                  )}
                </div>
              )}
            </div>
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
