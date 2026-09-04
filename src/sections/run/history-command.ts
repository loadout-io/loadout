/* `/history` — co już tutaj biegło i jak wrócić do jednego z tych biegów.
 *
 * PO CO TO ISTNIEJE. Zamówienie właściciela 2026-08-22: „powinna być opcja zapisu naszych sesji
 * i wyboru z historii, /history komenda np", i zaraz potem warunek: „pamiętaj że wszystko ma być
 * per workspace ta historia". Ekran pracy trzyma JEDNĄ żywą rozmowę na terminal
 * (`./feed/live.ts`), a ta rozmowa żyje w oknie: zamknięcie karty albo przeładowanie zabiera ją
 * w całości. Wszystko, co po biegu zostaje, leży na dysku (`docs/ARCHITECTURE.md` §8) i do dziś
 * nie było stąd ani jednej drogi do tych plików.
 *
 * DLACZEGO OBOK `run-command.ts` I `ask-command.ts`, A NIE W `entry/`. Bo to jest ta sama
 * rodzina i ma leżeć na tej samej półce: rozbiór linii, odmowy i lista, którą wiersz wejścia
 * tylko PRZEWOZI. Wiersz wejścia pokazuje odpowiedź, a nie podejmuje decyzji — to repo nie ma
 * jsdom, więc polityka zamknięta w komponencie byłaby kodem, którego nic nie sądzi.
 *
 * PER WORKSPACE JEST TU JEDNĄ LINIĄ i to jest cały warunek właściciela: zakres czytamy
 * z `activeWorkspace()` w chwili naciśnięcia, a bez zakresu ODMAWIAMY. Cichy powrót do katalogu,
 * pod którym wstała aplikacja, pokazywałby historię cudzego projektu pod nazwą tego, na który
 * człowiek patrzy — i nie miałby ani jednego sygnału, po którym dałoby się to zauważyć.
 *
 * CZEGO TU NIE MA: WZNOWIENIA. `/history` czyta i tylko czyta. Kontrolka „uruchom to jeszcze
 * raz" jest osobną decyzją produktową (co z folderem, co z limitem, co z krokami, które już
 * przeszły) i kontrolka bez tej decyzji byłaby kontrolką, która robi coś innego, niż mówi
 * (niezmiennik 16).
 */
import { why } from '../../ipc/why';
import { stepStateOf, type Step } from '../../state/run';
import { activeWorkspace } from '../../state/workspaces';
import type { PastRun, PastRunRow } from './io';
import { listRuns, readRun } from './io';
import { sayInHistory, showHistory, showPastRun, whatCouldGoIn } from './past/store';
import type { AgentStatus } from './rail/card';

/**
 * Co powiedzieć, kiedy w tym folderze jeszcze nic nie biegło.
 *
 * ZAPROSZENIE, NIE KOMUNIKAT O BRAKU DANYCH (DESIGN §6): pusta historia świeżego projektu jest
 * stanem normalnym, a zdanie „nothing found" zostawia człowieka dokładnie tam, gdzie był.
 */
export const NOTHING_YET = 'Nothing has run in this folder yet. Type /run to start something.';

/**
 * Co powiedzieć, kiedy nie ma folderu, o którego historię można by zapytać.
 *
 * OSOBNE ZDANIE OD `NO_FOLDER` z `./launch.ts`, choć oba mówią o tym samym braku. Tamto zaczyna
 * się od „Nothing started" i mówi o biegu, który nie ruszył; tutaj nic nie miało ruszyć, a
 * pytanie brzmiało „co tu było". Jedno zdanie na oba znaczyłoby, że człowiek pytający o historię
 * dostaje odpowiedź o Starcie. Wyjście jest w obu to samo i nazwane tym samym napisem, który
 * stoi na przycisku w bocznym menu.
 */
export const NO_FOLDER_TO_LOOK_IN =
  'History belongs to a folder, and none is chosen yet. Add a workspace in the side menu, then ' +
  'type /history again.';

/** Co powiedzieć, kiedy dysk odmówił odczytu listy. */
export const COULD_NOT_READ = 'Loadout could not read what has run in this folder.';

/** Co powiedzieć, kiedy dysk odmówił odczytu jednego biegu. */
export const COULD_NOT_OPEN = 'Loadout could not open that run.';

/**
 * Słowo z drutu → słowo, które czyta człowiek.
 *
 * PIĘĆ WARTOŚCI Z `RunState` PO STRONIE RUSTA (`commands::run`), przełożone na słownik, którym
 * ten ekran już mówi: [`AgentStatus`] z `./rail/card.ts`. Nie druga tabela — ta sama, bo kafelek
 * agenta i wiersz historii mówiące o „done" dwoma różnymi słowami czytają się jak dwa różne
 * stany (niezmiennik 13).
 *
 * Nieznane słowo oddaje pusty napis, i to jest niezmiennik 14 w najwęższym miejscu: enum z drutu
 * nigdy nie trafia na ekran, więc wartość, której nie umiemy przełożyć, nie ma prawa przejść
 * przez tę funkcję jako ona sama. Wiersz pokazuje wtedy resztę tego, co wie.
 */
type HistoryState = AgentStatus | 'not run';

export function stateWord(state: string): HistoryState | '' {
  switch (state) {
    case 'running':
      return 'working';
    case 'paused':
      return 'needs you';
    case 'succeeded':
      return 'done';
    case 'failed':
      return 'failed';
    case 'cancelled':
      return 'stopped';
    case 'not_run':
      // 2026-09 (Z-33) — to stan wyłącznie zapisanego kroku, nie żywego agenta. Dodanie go do
      // `AgentStatus` rozszerzyłoby maszynę stanu paska o wartość, której runtime nie produkuje.
      return 'not run';
    default:
      return '';
  }
}

/**
 * Kroki zapisanego biegu jako ten sam plan, który czyta produkcyjny obraz biegu.
 *
 * 2026-09 (Z-37) — Z PLIKU BIEGU, nie z dzisiejszego workflow: człowiek mógł po biegu
 * przepisać albo skasować jego plan. `not_run` jest słowem wyłącznie historii i odpowiada
 * żywemu `skipped`; nieznane słowo znika w ciszy, zamiast wejść na ekran jako enum z drutu
 * (niezmienniki 5 i 14).
 */
export function planOfPastRun(run: PastRun): readonly Step[] {
  const plan: Step[] = [];
  for (const past of run.steps) {
    const state = past.state === 'not_run' ? 'skipped' : stepStateOf(past.state);
    if (state === null) continue;
    plan.push({
      /* `tile` wskazuje kafelek; `id` jest tylko fizycznym UUID kroku tego biegu. Stary zapis
       * bez `tile` nadal potrzebuje stabilnego klucza, więc wtedy zostaje jedyny dostępny. */
      id: past.tile === '' ? past.id : past.tile,
      name: past.name,
      state,
    });
  }
  return plan;
}

/**
 * Ile ten bieg kosztował, do pokazania — albo pusty napis, kiedy nikt tego nie zmierzył.
 *
 * `null` znaczy „żaden krok nie podał kosztu", a to jest inne zdanie niż `$0.00` (niezmiennik 17):
 * zero mówi „nie kosztowało nic", a brak mówi „nie wiemy". Ten sam zapis, co w pasku loadoutu
 * (`./strip/model.ts`), żeby jedna liczba nie miała dwóch kształtów na jednym ekranie.
 */
export function costText(costUsd: number | null): string {
  return costUsd === null ? '' : '$' + costUsd.toFixed(2);
}

/**
 * Biegi pasujące do tego, co człowiek dopisał po `/history`.
 *
 * PUSTY DOPISEK ZNACZY „WSZYSTKO", a nie „nic": `/history` bez argumentu jest pytaniem o całą
 * historię i tak brzmi zachęta wiersza wejścia.
 *
 * Dopasowanie po NAZWIE WORKFLOW albo po dacie, bez rozróżniania wielkości liter, i to jest cały
 * zbiór rzeczy, które człowiek na tej liście WIDZI. Dopasowywanie po nazwie katalogu byłoby
 * dopasowywaniem po napisie, którego nie ma na ekranie — a filtr, którego wyniku nie da się
 * przewidzieć z tego, co widać, jest zgadywanką.
 */
export function matching(rows: readonly PastRunRow[], want: string): readonly PastRunRow[] {
  const looking = want.trim().toLowerCase();
  if (looking === '') return rows;
  return rows.filter(
    (row) => row.title.toLowerCase().includes(looking) || row.when.toLowerCase().includes(looking),
  );
}

/**
 * Co powiedzieć, kiedy dopisek nie pasuje do niczego.
 *
 * WYMIENIA, CO TU JEST, bo pytanie „którego" bez listy jest zagadką — dokładnie tak samo, jak
 * przy `/run` i `/ask` (`./run-command.ts`, `./ask-command.ts`). Najwyżej [`NAMED_AT_MOST`]
 * nazw: pełna lista przy czterdziestu biegach jest ścianą tekstu w miejscu, w którym miała być
 * podpowiedź.
 */
export function nothingLikeThat(want: string, rows: readonly PastRunRow[]): string {
  const names = rows
    .slice(0, NAMED_AT_MOST)
    .map((row) => (row.title === '' ? row.when : row.title))
    .join(', ');
  return (
    'Nothing here matches "' +
    want.trim() +
    '". These ran in this folder: ' +
    names +
    (rows.length > NAMED_AT_MOST ? ', and ' + String(rows.length - NAMED_AT_MOST) + ' more.' : '.')
  );
}

/** Ile nazw wchodzi do odmowy, zanim zacznie ich być za dużo do przeczytania. */
const NAMED_AT_MOST = 5;

/**
 * Który z tych biegów IDZIE — albo `null`, kiedy żaden.
 *
 * # Po co to istnieje
 *
 * Żeby okno, które dopiero wstało, mogło odnaleźć bieg pracujący od wcześniej. Pamięć okna
 * o żywym biegu jest ulotna: przeładowanie strony zeruje magazyny, a bieg po tamtej stronie
 * pracuje dalej — i wtedy ekran nie ma ani paska, ani Stopu nad czymś, co kosztuje pieniądze
 * (zgłoszenie właściciela 2026-08-23).
 *
 * # Dlaczego to jest funkcja, a nie `rows.find` w komponencie
 *
 * Bo to jest jedyne zdanie, które odróżnia bieg PRACUJĄCY od biegu, który tylko tak wygląda —
 * a takich słów na drucie jest więcej niż jedno. `interrupted` niesie bieg porzucony przez
 * zamknięte okno; `paused` niesie bieg stojący na pytaniu, który dalej trzyma uchwyt. Warunek
 * wpisany w komponent nie ma jak być osądzony, bo to repo nie ma jsdom.
 *
 * PIERWSZY, nie „jedyny": `list_runs` oddaje od najnowszego, a Loadout prowadzi jeden bieg
 * naraz — więc drugi wiersz ze słowem `running` znaczyłby, że coś już poszło nie tak, i nie
 * jest to miejsce, żeby o tym decydować.
 */
export function theOneThatIsGoing(rows: readonly PastRunRow[]): PastRunRow | null {
  return rows.find((row) => row.state === 'running') ?? null;
}

/**
 * `/history [słowo]` z wiersza wejścia: pokazuje historię TEGO folderu.
 *
 * Oddaje zdanie na ekran albo `null`, kiedy panel się otworzył — czyli dokładnie ten sam
 * kontrakt, co `startFromLine` i `startAskFromLine`. `null` znaczy „skutek widać", a widać go
 * w panelu, który właśnie stanął.
 *
 * DYSK CZYTAMY TERAZ, przy naciśnięciu, a nie z listy zapamiętanej przy renderze: pliki są
 * prawdą (niezmiennik 4), a bieg mógł skończyć się sekundę temu.
 */
export async function openHistoryFromLine(rest: string): Promise<string | null> {
  const folder = activeWorkspace()?.folder ?? null;
  if (folder === null) return NO_FOLDER_TO_LOOK_IN;

  let rows: readonly PastRunRow[];
  try {
    rows = await listRuns(folder);
  } catch (error: unknown) {
    return why(error, COULD_NOT_READ);
  }

  if (rows.length === 0) {
    /* PUSTA LISTA NIE ZNACZY PUSTY FOLDER (2026-09, Z-46). Do tego dnia `/history` odpowiadał tu
     * zaproszeniem i panelu nie otwierał — a to jest dokładnie ten stan, do którego prowadzi
     * „Forget runs older than …": katalogi biegów schodzą, gałęzie po nich zostają, i wtedy nie ma
     * już ŻADNEJ drogi, którą człowiek mógłby te gałęzie zobaczyć albo zdjąć. Zmierzone
     * u właściciela na jednym monorepo: 99 gałęzi `loadout/*` przy czternastu biegach.
     *
     * Odpowiedź jedzie do `showHistory` ARGUMENTEM: panel pyta o to samo przy otwarciu, a dwa
     * odczyty na jedno naciśnięcie dałyby dwie odpowiedzi z dwóch chwil (niezmiennik 13). */
    const could = await whatCouldGoIn(folder);
    if (could === null || could.said === '') return NOTHING_YET;
    showHistory(folder, [], could);
    return null;
  }

  const wanted = matching(rows, rest);
  if (wanted.length === 0) return nothingLikeThat(rest, rows);

  showHistory(folder, wanted);
  return null;
}

/**
 * Wiersz listy kliknięty: otwiera TEN bieg do odczytu.
 *
 * Wołane z wiersza panelu (`./past/panel.tsx`) i tylko stamtąd. Zdanie odmowy ląduje w panelu,
 * nie w strumieniu — powód stoi przy `sayInHistory`.
 *
 * Zakres jedzie ARGUMENTEM, z magazynu panelu, a nie z `activeWorkspace()`: lista przyszła
 * z konkretnego folderu, więc wiersz na niej ma być czytany z tego samego folderu, także wtedy,
 * gdy człowiek przełączył boczne menu, zanim kliknął.
 */
export async function openOneRun(folder: string | null, run: string): Promise<void> {
  try {
    showPastRun(await readRun(folder, run));
  } catch (error: unknown) {
    sayInHistory(why(error, COULD_NOT_OPEN));
  }
}
