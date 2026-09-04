/* Wiersz propozycji po stronie okna: co przycisk mówi i co robi.
 *
 * PO CO TO JEST OSOBNY PLIK. Bo to jedyne dwie rzeczy, które okno ma jeszcze do zrobienia
 * z propozycją, i obie są polityką, nie rysowaniem: jak nazywa się workflow, który się
 * uruchomi, i którędy uruchomienie idzie. Zamknięte w komponencie byłyby kodem, którego żadne
 * kryterium nie umie dotknąć — to repo nie ma jsdom, więc `onClick` nie odpala się w teście
 * (`start-invokes.test.tsx`, nagłówek). Ta sama rodzina, z której wzięło się siedemnaście
 * kłamiących kontrolek w repo źródłowym.
 *
 * CZEGO TU NIE MA I MIEĆ NIE MOŻE: rozpoznawania propozycji. Czy proza lidera nią jest,
 * rozstrzyga Rust, w mapowaniu zdarzenie -> linia (`engine::line::suggested`, niezmiennik 15).
 * Okno, które samo szuka `/run` w prozie agenta i dorysowuje przycisk, jest kuracją w CSS-ie:
 * da się ją zepsuć arkuszem stylów, nie da się jej sprawdzić bez przeglądarki i nie ma jej
 * w `run.json`. Ten plik dostaje komendę, którą wiersz PRZYNIÓSŁ, i nic z niej nie odgaduje
 * poza jej pierwszym słowem.
 *
 * DLACZEGO START IDZIE PRZEZ `startFromLine`, A NIE PROSTO DO `launchRun`. Bo „który workflow,
 * ile naraz, w którym folderze" ma jedną odpowiedź (niezmiennik 23), a `startFromLine` jest tą
 * odpowiedzią: czyta katalog workflow w chwili kliknięcia, bierze limit z `./limits/chosen` —
 * czyli z tego samego modułu, z którego czyta go suwak obok Startu — i oddaje zdanie odmowy.
 * Druga droga startu byłaby drugą odpowiedzią, a pierwszy rozjazd między nimi jest cichy:
 * liczba jest wczytywana, logowana i inna.
 *
 * CZEGO TU NIE MA I MIEĆ NIE MOŻE, RAZ JESZCZE: własnego limitu „ile naraz", własnego odczytu
 * katalogu workflow i własnego zdania odmowy o nieistniejącym workflow. Wszystkie trzy pisze
 * `startFromLine`, a druga ich kopia rozjeżdża się po cichu — liczba jest wczytywana, logowana
 * i inna. Jedyne zdanie, które powstaje TUTAJ, dotyczy linii, która komendą nie jest.
 */
import { whatStopSaid } from '../entry/entry';
import { folderName, whereTheRunIs } from '../folders';
import { stop } from '../io';
import { startFromLine } from '../run-command';

/** Komenda, którą zaczyna się bieg — to samo słowo, co po stronie Rusta i w wierszu wejścia. */
const RUN = '/run';

/**
 * Komenda, którą bieg się kończy — to samo słowo, co w wierszu wejścia.
 *
 * 2026-09 (Z-39) — DOSZŁA, BO LIDER DOSTAŁ CZASOWNIK `stop_run`. Do tego dnia most nie miał ani
 * jednej drogi do zatrzymania biegu, więc lider zatrzymywał go tak, jak potrafił: czytał `pgid`
 * z `run.json` i wołał `kill` narzędziem Bash (bieg meetnotes `20260901-150035`). Wiersz z mostu
 * przychodzi tą samą krawędzią, co start — bez tej gałęzi odbiłby się od `runSuggestion` zdaniem
 * „That line does not name a workflow".
 */
const STOP = '/stop';

/**
 * Co powiedzieć, kiedy w komendzie nie da się znaleźć nazwy workflow.
 *
 * DROGA, KTÓRA TU PROWADZI, JEST WĄSKA I DLATEGO TO ZDANIE ISTNIEJE. Wiersz propozycji powstaje
 * w Ruście dokładnie z linii, która ma tę postać (`engine::line::suggested`), więc normalną
 * drogą nie dojdzie tu nic innego. Lustro drutu sprawdza jednak tylko TYP tego pola
 * (`command` to napis), więc linia z innej wersji Rusta jest kształtem, który okno przyjmie —
 * a cisza po naciśnięciu przycisku czyta się jak zepsuta aplikacja (DESIGN §8).
 */
const NAMES_NO_WORKFLOW =
  'That line does not name a workflow, so there is nothing to start. Type /run and the name in ' +
  'the command line below.';

/** Co proponuje komenda z wiersza. */
export interface Suggestion {
  /**
   * Nazwa workflow — pierwsze słowo po `/run`.
   *
   * Do nazwy przycisku, i to jest cała treść tego pola: „Run" bez nazwy nie mówi, co się
   * stanie, a przycisk, który nie mówi, co uruchomi, jest pytaniem, nie kontrolką.
   */
  readonly workflow: string;
  /**
   * Reszta linii po `/run`, znak w znak — dokładnie to, co dostaje polityka startu.
   *
   * Ten sam napis, który jedzie z wiersza wejścia po naciśnięciu Enter (`entry.tsx`:
   * `typed.slice('/run'.length).trim()`). Gdyby te dwie drogi podawały politykę różne napisy,
   * jeden z nich byłby drugą odpowiedzią na pytanie „co ma się uruchomić".
   */
  readonly rest: string;
}

/**
 * Co niesie ta komenda — albo `null`, kiedy to nie jest komenda.
 *
 * `null` jest tu odpowiedzią, nie wyjątkiem: wiersz, którego Rust nie uznał za propozycję,
 * nie dojedzie tu nigdy, a widok wywrócony na jednej linii traci CAŁY strumień, nie tę linię
 * (niezmiennik 5 w duchu, po stronie okna).
 *
 * Wołający: `./line.tsx`, po nazwę przycisku. Do fazy implementacji nie ma go — i to jedyna
 * rzecz w tym pliku, której żadne kryterium nie wymaga wprost. Stoi tu, bo alternatywą jest
 * rozbiór komendy wpisany w komponent, czyli polityka w miejscu, którego test nie dotknie.
 */
export function suggestion(command: string): Suggestion | null {
  const line = command.trim();
  if (!line.startsWith(RUN)) return null;

  const after = line.slice(RUN.length);
  /* `/runner easy` nie jest `/run`: bez białej spacji po komendzie pierwszym słowem byłaby
   * nazwa, której nikt nie napisał. Ten sam warunek stoi po stronie Rusta (`names_a_workflow`),
   * bo to jedno pytanie i ma mieć jedną odpowiedź na obu brzegach granicy. */
  if (after !== '' && !/^\s/.test(after)) return null;

  /* PRZYCIĘTA RESZTA LINII, ZNAK W ZNAK TA SAMA, KTÓRĄ PODAJE ENTER: `entry.tsx` liczy ją jako
   * `typed.trim().slice('/run'.length).trim()`. Gdyby te dwie drogi podawały polityce różne
   * napisy, jeden z nich byłby drugą odpowiedzią na pytanie „co ma się uruchomić", a rozjazd
   * między nimi jest cichy — bieg startuje, tylko nie ten. */
  const rest = after.trim();
  const workflow = rest.split(/\s+/)[0] ?? '';
  /* Bez nazwy nie ma czego napisać na przycisku, a „Run" bez nazwy nie mówi, co się stanie —
   * więc to nie jest propozycja, którą wolno komukolwiek pokazać jako kontrolkę. */
  if (workflow === '') return null;
  return { workflow, rest };
}

/**
 * Kliknięcie: uruchamia propozycję TĄ SAMĄ drogą, co Enter w wierszu wejścia.
 *
 * Oddaje zdanie odmowy albo `null`, kiedy bieg poszedł — kształt `startFromLine`, znak w znak,
 * bo to jest ta sama odpowiedź i ma być pokazana w ten sam sposób. Odmowa porzucona po drodze
 * jest gorsza niż brak przycisku: człowiek klika i nie dzieje się nic, o czym da się przeczytać.
 *
 * @param folder katalog rozmowy, z której ta propozycja przyszła, albo `null`. Jedzie tylko do
 *   `/stop`, bo tylko on ADRESUJE bieg — start pyta o folder politykę (`startFromLine`), tak jak
 *   pyta ją o wszystko inne (niezmiennik 23). Wartość domyślna zostaje, bo przycisk pod wierszem
 *   woła tę funkcję jednym argumentem i nie ma czym tego folderu podać.
 */
export async function runSuggestion(
  command: string,
  folder: string | null = null,
): Promise<string | null> {
  /* PRZED `suggestion`, bo tamta zna wyłącznie `/run` i każde inne słowo oddaje jako `null`,
   * czyli jako „to nie jest komenda". Rozbioru `/stop` nie ma, bo nie ma czego rozbierać:
   * cała jego treść to adres, a adres przychodzi argumentem. */
  if (command.trim() === STOP) {
    const where = whereTheRunIs(folder);
    /* JEDNO ŹRÓDŁO ZDANIA O STOPIE (niezmiennik 13): to samo, którym odpowiada wiersz wejścia
     * i przycisk Stop. `null` znaczy „zeszło" i wtedy nie ma czego pokazywać. */
    return whatStopSaid(await stop(where), where === null ? null : folderName(where));
  }
  const proposal = suggestion(command);
  if (proposal === null) return NAMES_NO_WORKFLOW;
  /* JEDNO WYWOŁANIE I ANI JEDNEJ GAŁĘZI OBOK NIEGO. Wszystko, co jeszcze trzeba rozstrzygnąć —
   * czy ten workflow leży na dysku, ile agentów naraz, w jakim folderze i co powiedzieć, kiedy
   * któraś z tych odpowiedzi jest odmowna — należy do polityki startu (niezmiennik 23). Zdanie
   * odmowy oddajemy dalej takie, jakie wróciło: przepisane tutaj byłoby drugą odpowiedzią na
   * pytanie, na które tamta funkcja już odpowiedziała, i to tą, która nie zna nazw z dysku. */
  return startFromLine(proposal.rest);
}
