# T-14 — Lista workflow: utwórz, zduplikuj, usuń

Małe zadanie z jedną prawdziwą pułapką. Nazwa workflow zamienia się na nazwę pliku, a dwie różne
nazwy potrafią dać ten sam plik: `Ship a feature` i `Ship a Feature` to jeden `ship-a-feature.json`.
Cicha porażka wygląda tak, że drugi zapis kończy się sukcesem, lista pokazuje dwie pozycje, a na dysku
jest jedna — i użytkownik traci workflow, którego nigdy nie usuwał. Druga pułapka jest wizualna:
makieta pokazuje na kafelku `used 12×` i `~6 min`, czyli dane z historii biegów, których v1 jeszcze
nie ma. Wyrenderowanie ich jako `—` albo `never` to dokładnie ten anty-wzorzec, który poprzedni prototyp
zostawił po sobie w postaci komórki `SPEND: not reported` (00-SYNTHESIS §6): pole, które nigdy nie
będzie miało treści, zajmujące miejsce na ekranie i tłumaczące się użytkownikowi z własnej pustki.

**Read first:** `docs/mockup/index.html` linie 576–597 (wiążący układ ekranu: nagłówek `Workflows`,
licznik `3 saved`, przycisk `＋ Create`, kafelki z jednym zdaniem opisu i wierszem metadanych),
`docs/research/topics/T3-workflow-editor.md` §3.1 (pola `id` i `name` — `id` jest stabilny i **nigdy
nie zmienia się przy zmianie nazwy**, co rozstrzyga, czy plik da się przemianować) i §8.3 (jedna
ścieżka szukania: `~/.loadout/workflows/<slug>.json`; dwie ścieżki znaczą reguły pierwszeństwa i błędy
pierwszeństwa), `docs/design/DESIGN.md` §6 `empty-state` (pusty ekran to zaproszenie do działania,
nie komunikat o braku danych) i §8 (`Nothing here yet.` zamiast `No records found`).

## Kto to robi

- **Agent:** `react-ui`
- **Druga opinia:** Codex — pisze Claude Code (D3). Recenzent nie zatwierdza i nie blokuje;
  niedostępny recenzent to `exit 0` z notatką.
- **Artefakty biegu:** `runs/T-14/`

**Czym testujemy.** W repo **nie ma** `jsdom` ani `@testing-library/react`, a `package.json`
i `vite.config.ts` są na liście `DENIED` w `checks/quick-scope.sh` (to samo ograniczenie mają T-08
i T-09). `vitest` biegnie w środowisku `node`, więc: logikę — czystymi funkcjami magazynu wołanymi
wprost, a to, co widzi użytkownik — przez `renderToStaticMarkup` z `react-dom/server`. Komponenty są
**sterowane**: stan i akcje przychodzą propsami, żadne kryterium nie potrzebuje zdarzenia myszy.

## Co to zadanie posiada

- `src/sections/workflows/list/**` — ekran listy, kafelek, przepływ tworzenia, potwierdzenie usunięcia,
  pusty stan oraz **własny** magazyn listy (`createWorkflowListStore(io: WorkflowListIo)`).
  Magazyn otwartego dokumentu (`src/state/workflows.ts`) należy do T-13 i tego zadania nie dotyczy:
  to są dwie różne rzeczy — spis plików i jeden otwarty plik.
- Pliki testowe wymienione przy `check:`.

Dwa rozstrzygnięcia, żeby implementacja nie zgadywała:

1. **Nazwa pliku powstaje raz, przy tworzeniu, i nigdy się nie zmienia.** `id` jest stabilny [T3 §3.1],
   więc zmiana nazwy workflow zmienia pole `name` i zostawia `ship-a-feature.json` tam, gdzie był.
   Przemianowywanie plików to operacja, która potrafi zgubić dane, i nic nam nie kupuje.
2. **Kafelek pokazuje wyłącznie to, co jest w pliku.** Nazwa, jedno zdanie opisu, `N steps`, `M agents`
   (liczba **różnych** identyfikatorów agentów w krokach). `used 12×` i `~6 min` z makiety wymagają
   historii biegów (T-06) i wchodzą razem z nią — nie jako pusta komórka.

## Niezmienniki

- **16 — kontrolka bez handlera nie wchodzi do repo.** Przycisk w pustym stanie i przycisk w nagłówku
  wołają **tę samą** funkcję. Dwa przyciski, jeden przepływ; drugi przepływ to drugie miejsce, w którym
  powstaje plik, i pierwsza okazja do rozjazdu.
- **13 — jeden fakt, jedno miejsce.** Licznik `3 saved` w nagłówku jest wyliczany z długości listy.
  Osobne pole w stanie rozjedzie się po pierwszym usunięciu.
- **14 — zero żargonu w tekście widocznym dla użytkownika.** `Create`, `Duplicate`, `Delete`,
  `4 steps`. Nigdy `instantiate`, `clone`, `remove entity`, `DAG`.
- **20 — test sprawdza zachowanie, nie obecność stringa.** „Na ekranie jest słowo Delete" nie jest
  dowodem, że coś się usuwa — ani że pytanie zadano przed usunięciem.
- **4 — pliki są prawdą.** Lista jest widokiem na katalog `~/.loadout/workflows/`. Usunięcie pozycji
  z listy bez usunięcia pliku daje stan, który wraca po restarcie.

## Kryteria akceptacji

Bramka odrzuca czerwień pochodzącą z braku modułu (`NOT_A_REAL_RED`), więc najpierw plik testowy
**i** moduł ze stubem rzucającym `new Error('not implemented')`; wtedy `before` pada w czasie
wykonania, a nie przy rozwiązywaniu importu.

## AC-1 Dwie różne nazwy nigdy nie trafiają do jednego pliku
check: npx --no-install vitest run src/sections/workflows/list/create.test.ts

`create('Ship a feature')` zapisuje `ship-a-feature.json` z `format: 1`, `steps: []`, `links: []`
i `name` dokładnie takim, jaki wpisał człowiek. Potem `create('Ship a Feature')` (inna wielkość liter,
ten sam slug): powstaje **drugi** plik o innej nazwie — `ship-a-feature-2.json` — a treść pierwszego
w atrapie systemu plików jest niezmieniona. Trzeci przypadek: nazwa złożona ze znaków, które ze slugu
nic nie zostawiają (`"???"`), też daje jeden zapisywalny, unikalny plik, a nie pusty `.json`.
Lista po tych operacjach jest posortowana po nazwie **bez uwzględnienia wielkości liter**: `apple`
stoi przed `Banana`.

*Słaba asercja:* `expect(io.write).toHaveBeenCalledTimes(2)`. Przechodzi, gdy drugi zapis nadpisał
pierwszy plik — dwa wywołania, jedna ścieżka, jeden workflow mniej. Dyskryminuje: porównanie zbioru
ścieżek przekazanych do `io.write` (dwa różne wpisy) oraz odczyt pierwszego pliku po drugim zapisie
i asercja, że dalej ma swoją nazwę. Sortowanie: `['apple','Banana']` — domyślne `Array.sort()` daje
`['Banana','apple']`, bo wielkie litery mają niższe kody.

## AC-2 Duplikat jest osobnym plikiem, a nie drugą nazwą tego samego obiektu
check: npx --no-install vitest run src/sections/workflows/list/duplicate.test.ts

`duplicate(id)` na workflow z dwoma krokami: powstaje wpis z **innym** `id`, nazwą `Deep research (copy)`
i własną ścieżką pliku. Identyfikatory kroków wewnątrz kopii zostają takie same — są lokalne dla pliku.
Kluczowy przypadek: po `copy.steps[0].name = 'Zmienione'` krok oryginału ma dalej swoją pierwotną
nazwę, a po `copy.links.push(...)` długość `links` oryginału się nie zmienia. Kopia jest głęboka.

*Słaba asercja:* `expect(list).toHaveLength(2)`. Przechodzi dla `list.push({...wf, id: newId})` —
płytka kopia, wspólna tablica `steps`, i pierwsza edycja duplikatu po cichu przepisuje oryginał, na
którym użytkownik pracuje od miesiąca. Dyskryminuje: mutacja `steps` i `links` kopii z asercją na
stanie oryginału.

## AC-3 Usunięcie pyta, nazywa po imieniu i po anulowaniu nie robi nic
check: npx --no-install vitest run src/sections/workflows/list/delete.test.tsx

Trzy czyste akcje magazynu i jeden render. `requestDelete(id)` ustawia pytanie i **nie** woła
`io.remove` ani razu; statyczny HTML tego stanu zawiera nazwę tego workflow i zdanie mówiące, co znika
(`Delete "Deep research"? The file goes away. Runs you already did stay.`). `cancelDelete()`:
`io.remove` dalej zero wywołań, lista dalej trzy pozycje, pytania w HTML już nie ma.
`confirmDelete()`: `io.remove` dokładnie raz i ze ścieżką **tego** pliku, a na liście zostają dwie
pozycje o oczekiwanych nazwach (asercja na nazwach, nie na długości).

*Słaba asercja:* `expect(io.remove).toHaveBeenCalled()` po potwierdzeniu. Przechodzi dla implementacji,
która usuwa plik **przed** pokazaniem pytania — pytanie staje się wtedy ozdobą, a `Cancel` kłamie.
Dyskryminuje: `expect(io.remove).not.toHaveBeenCalled()` w stanie, w którym pytanie jest już
wyrenderowane, oraz to samo po `cancelDelete()`.

## AC-4 Kafelek pokazuje wyłącznie fakty z pliku i nie zostawia pustych komórek
check: npx --no-install vitest run src/sections/workflows/list/tile.test.tsx

`renderToStaticMarkup(<WorkflowTile wf={…} />)` dla workflow z czterema krokami o dwóch różnych
identyfikatorach agentów zawiera `4 steps` i `2 agents`. Dla workflow z jednym krokiem zawiera
`1 step` i `1 agent` — liczba pojedyncza, nie `1 steps`. Workflow bez opisu **nie renderuje** pustego
akapitu (brak `<p></p>` w HTML). I najważniejsze: HTML kafelka nie pasuje do
`/used|min|never|—|not reported/`, bo historii biegów w v1 nie ma (makieta
`docs/mockup/index.html:583-591` pokazuje `used 12×` i `~6 min`; wchodzą razem z T-06).

*Słaba asercja:* `expect(html).toContain('4 steps')`. Przechodzi dla napisu wpisanego na stałe
w komponencie i przechodzi obok kafelka, który pod spodem renderuje `used —`. Dyskryminuje: przypadek
jednego kroku (liczba pojedyncza wyklucza `${n} steps` bez odmiany) oraz asercja negatywna na cały HTML.

## AC-5 Pusty ekran jest zaproszeniem, a jego przycisk naprawdę tworzy
check: npx --no-install vitest run src/sections/workflows/list/empty-state.test.tsx

Render listy ze stanem `workflows: []` daje jedno zdanie po angielsku będące zaproszeniem
(`No workflows yet.` plus jedno zdanie instrukcji) i **dokładnie jeden** przycisk podstawowy; nie ma
nagłówków tabeli, nie ma pustej siatki, nie ma zdania o braku danych (DESIGN §6, §8). Render tego
samego ekranu z jednym workflow: zaproszenia w HTML nie ma. Przycisk nie jest martwy — obie ścieżki
tworzenia (pusty stan i nagłówek) dostają **ten sam** obiekt `actions`, a `actions.create('Ship it')`
wołane w teście naprawdę zapisuje plik przez atrapę `io` i dokłada pozycję do listy.

*Słaba asercja:* `expect(html).toContain('No workflows yet')`. Przechodzi dla ekranu z martwym
przyciskiem — poprzedni prototyp ma trzy takie (00-SYNTHESIS §6, „dead controls with no onClick").
Dyskryminuje: wywołanie `actions.create` z asercją na zapisie w atrapie `io` oraz render z jednym
workflow, w którym zaproszenia już nie ma.

## Świadomie poza zakresem

- **`used 12×` i `~6 min`** — wymagają historii biegów z T-06. Wchodzą razem z nią, nigdy jako `—`.
- **Zmiana nazwy workflow.** Nazwa jest edytowalna na płótnie (T-13); tu tylko wyświetlamy.
  Plik nie jest przemianowywany nigdy — patrz rozstrzygnięcie 1.
- **Kopiowanie workflow między maszynami, eksport do zipa razem z agentami** — T4 §5.3 opisuje to
  jako folder plików tekstowych; osobne zadanie, kiedy będzie z kim się dzielić.
- **Szukanie i filtrowanie listy.** Przy kilkunastu workflow to szum. Wchodzi, kiedy ktoś naprawdę
  przewinie tę listę.
- **Repozytoryjne workflow (`<repo>/.loadout/workflows/`)** — świadomie odłożone [T3 §8.3].
- **Walidacja treści plików.** Robi ją Rust (T-12). Lista pokazuje plik, którego nie umiała wczytać,
  jako pozycję z jednym zdaniem błędu — nie ukrywa go i nie próbuje naprawić.

<!-- OWNS
src/sections/workflows/list
-->
