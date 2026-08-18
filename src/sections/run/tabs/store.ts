/* Karty paska na ekranie Run — od 2026-08-18 karty BIEGÓW, nie folderów.
 *
 * CO SIĘ ZMIENIŁO I DLACZEGO. Do dziś karta znaczyła folder: `＋` otwierał okno wyboru
 * katalogu, a karta na wierzchu niosła odpowiedź na pytanie „gdzie pracują agenci". Właściciel
 * rozstrzygnął to inaczej (2026-08-18): projekt wybiera się w bocznym menu jako ZAKRES, raz,
 * a pasek kart w środku ekranu pokazuje **biegi w tym zakresie**. Poprzedni plik — dawne
 * `src/sections/run/workspaces-store.ts` — modelował karty folderów i był zarazem jedynym
 * miejscem, w którym mieszkało „gdzie pracujemy"; oba te zadania mu odebrano: pierwsze przeszło
 * tutaj w nowym znaczeniu, drugie do `activeWorkspace()` w `src/state/workspaces.ts`.
 *
 * ILE KART MOŻE BYĆ — i to jest liczba z danych, nie z makiety. Karta to jeden bieg, a bieg
 * należy do dokładnie jednego zakresu; zakres to dokładnie jeden folder. Dwa biegi w jednym
 * folderze pisałyby po tych samych plikach, czego to repo odmawia już przy zapisie workflow
 * (niezmiennik 12), więc **w jednym zakresie może stać najwyżej jedna karta**, i tak jest
 * dopóty, dopóki bieg nie ma tożsamości na drucie. Pasek pokazujący trzy karty „bo makieta ma
 * trzy" rysowałby relację, której w danych nie ma (niezmiennik 17). Stąd `id` karty JEST
 * folderem biegu: jedno pole odpowiada na pytania „która to karta" i „gdzie ten bieg pracuje",
 * zamiast dokładać odwzorowanie między dwoma identyfikatorami (niezmiennik 13).
 *
 * MAGAZYN NA POZIOMIE MODUŁU, bo bieg trwa dłużej niż ekran: wyjście do Agentów odmontowuje
 * komponent i nie ma prawa zgubić kart ani skasować biegu.
 *
 * FABRYKA MIESZKA W `src/state/run-tabs.ts` i tam została po przeprowadzce. Ten plik jest
 * egzemplarzem tego okna plus dwiema czynnościami, których fabryka mieć nie może, bo obie
 * dotykają granicy: zatrzymanie biegu (`stop`) i założenie karty w chwili startu.
 */
import { createWorkspacesStore } from '../../../state/run-tabs';
import type { WorkspaceTab, WorkspacesStore } from '../../../state/run-tabs';
import { runFor } from '../../../state/run';
import { stop } from '../io';

/**
 * Zatrzymuje bieg **tej** karty — i milczy o cudzym.
 *
 * 2026-08-18 — DEFEKT, KTÓRY TA FUNKCJA ZAMYKA I DALEJ ZAMYKA. Domknięcie stało kiedyś jako
 * `() => stop()`: identyfikator karty był ignorowany, choć typ `CancelRun` go daje. Przy jednej
 * karcie było to niewidoczne, a przy drugiej znak `×` na karcie, w której nic nie chodzi, ubijał
 * bieg idący gdzie indziej — cudzą pracę, bez pytania i bez śladu.
 *
 * Rozstrzygamy tym, co okno WIE: `id` karty jest folderem jej biegu, a sesja tego folderu
 * (`runFor`) niesie nazwę workflow dokładnie wtedy, kiedy w tym folderze coś idzie. Silnik
 * prowadzi dziś jeden bieg naraz (zapadka `going` w `../io`), więc „w tym folderze coś idzie"
 * jest równoważne „to jest TEN bieg, który zatrzyma `stop_run`" — i to jest cała uczciwość,
 * jaką ta funkcja może mieć bez argumentu po tamtej stronie granicy.
 *
 * DALEJ ZATRZYMUJEMY, kiedy sesja bez zakresu (klucz `null`) ma żywy bieg: tak wygląda bieg
 * puszczony przez `start()` bez folderu, czyli tam, gdzie wstała aplikacja. Nie wiemy, czyj
 * jest, a osierocony agent palący limit jest gorszy niż zatrzymanie o jedno za dużo
 * (niezmiennik 6).
 *
 * PRAWDZIWA NAPRAWA JEST PO STRONIE RUSTA i jest zgłoszona: `stop_run` nie bierze
 * identyfikatora, więc okno prowadzące dwa biegi naraz nadal nie miałoby czym wybrać. Ta
 * funkcja nie udaje, że go ma — po prostu nie zabija biegu, o którym wie, że nie należy do
 * zamykanej karty.
 */
async function stopRunOf(tab: string): Promise<void> {
  if (runFor(tab).getState().workflow === '' && runFor(null).getState().workflow === '') return;
  await stop();
}

/**
 * Karty biegów tego okna.
 *
 * ZATRZYMANIE WCHODZI ARGUMENTEM i dziś jest nim `stopRunOf` — `stop_run` obwarowany pytaniem
 * „czy ten bieg w ogóle należy do tej karty". Dzień, w którym `stop_run` dostanie identyfikator
 * biegu, jest dniem, w którym zmienia się dokładnie ta jedna linia.
 */
export const runTabs: WorkspacesStore = createWorkspacesStore(stopRunOf);

/**
 * Zakłada (albo odświeża) kartę biegu, który właśnie rusza, i stawia ją na wierzchu.
 *
 * WOŁANE Z JEDNEGO MIEJSCA — `../launch`, czyli z tej samej polityki, która decyduje, czy bieg
 * w ogóle wystartuje. Karta założona gdziekolwiek indziej byłaby kartą biegu, który mógł się
 * nie zacząć, a to jest kropka „tu coś chodzi" nad folderem, w którym nic nie chodzi
 * (niezmiennik 17).
 *
 * NAZWA JEST PISANA WPROST, nie przez `open`, i to jest zapisany dług. `open` z fabryki nie
 * dotyka karty, która już stoi (jeden folder = jedna karta), więc drugi bieg — innego workflow,
 * w tym samym zakresie — zostawiałby na karcie nazwę poprzedniego. Fabryka nie ma akcji
 * „zmień nazwę", a `src/state/run-tabs.ts` nie należy do tego zadania: zgłoszone
 * orkiestratorowi. Zapis idzie przez `setState` samego magazynu, czyli po jego własnym
 * interfejsie, i dotyka JEDNEGO pola.
 */
export function cardForRun(workflow: string, folder: string): void {
  const { tabs } = runTabs.getState();
  const already = tabs.find((tab) => tab.id === folder);
  if (already === undefined) {
    runTabs.getState().open({ id: folder, name: workflow, path: folder, agents: 0 });
    return;
  }
  runTabs.setState({
    tabs: tabs.map((tab) => (tab.id === folder ? { ...tab, name: workflow } : tab)),
    activeId: folder,
  });
}

/**
 * Karty, które należą do tego zakresu — czyli te, które pasek ma pokazać.
 *
 * Pasek pokazuje biegi zakresu, w którym człowiek stoi (rozstrzygnięcie właściciela: switcher
 * w bocznym menu ORAZ karty w środku). Bieg z innego zakresu nie znika i nie zwalnia — ma
 * tylko swoją kartę tam, gdzie pracuje.
 *
 * BEZ ZAKRESU NIE FILTRUJEMY, i to jest wybór, nie przeoczenie. Filtr wymaga zakresu; okno bez
 * ani jednego zakresu nie ma jak zacząć biegu (`launchRun` odmawia), więc karta może tam stać
 * tylko z biegu puszczonego wcześniej albo z testu — a karta ukryta jest kartą, której `×` nie
 * da się nacisnąć, czyli biegiem, którego nie da się zatrzymać (niezmiennik 6).
 */
export function cardsIn(
  tabs: readonly WorkspaceTab[],
  folder: string | null,
): readonly WorkspaceTab[] {
  if (folder === null) return tabs;
  return tabs.filter((tab) => tab.id === folder);
}
