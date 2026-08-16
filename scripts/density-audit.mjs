/* Sufit gęstości: kolektor i sędzia, rozdzieleni celowo.
 *
 * SZKIELET FAZY KONTRAKTU. Sygnatury są prawdziwe, ciał jeszcze nie ma. To jest dokładny
 * odpowiednik `todo!()` z Rusta i istnieje z tego samego powodu (AGENTS.md §2a): moduł,
 * którego vitest nie umie wczytać, daje "Cannot find module" — podpis z listy
 * `NOT_A_REAL_RED`, czyli czerwień, która nic nie poświadcza. Import ma się rozwiązać,
 * a specyfikacja ma paść na ZACHOWANIU.
 *
 * Podział, który to zadanie ustala i który jest powodem, dla którego ten plik w ogóle
 * istnieje osobno od `checks/quick-density.sh`:
 *
 *   KOLEKTOR biegnie w przeglądarce i mierzy DOM przy dwóch szerokościach okna —
 *   1100 px (najwęższa wspierana, docs/design/DESIGN.md §9) i 1512 px [03 §4.1].
 *   Nie ma kryterium akceptacji i mieć nie może: `NOT_A_REAL_RED` zawiera
 *   "Failed to launch" i "Executable doesn't exist", więc kryterium wymagające Chromium
 *   na maszynie bez pobranych przeglądarek daje w `before` czerwień, którą bramka odrzuca,
 *   a w `full` zieleń, która nic nie znaczy. Repo źródłowe scertyfikowało tak siedem
 *   kryteriów na przeglądarce, która nie startowała [03 §4.1].
 *
 *   SĘDZIA jest czystą funkcją nad zrzutem JSON. Żadnego okna, żadnego wejścia-wyjścia.
 *   Dlatego to on ma kryteria (AC-5, AC-6) i dlatego daje się przetestować na fiksturach.
 *
 * Sufit ma JEDNĄ kopię i mieszka w docs/ARCHITECTURE.md §7. `readCeiling` go PARSUJE.
 * Przepisanie siedmiu liczb tutaj byłoby drugą kopią i po pierwszej edycji dokumentu
 * bramka pilnowałaby liczby, której już nikt nie deklaruje (niezmiennik 18).
 */

/**
 * Siedem wpisów sufitu, w kolejności wierszy tabeli §7.
 *
 * @param {string} path ścieżka do docs/ARCHITECTURE.md
 * @returns {Array<{key: string, label: string, limit: number}>}
 */
export function readCeiling(path) {
  void path;
  throw new Error('readCeiling is not implemented yet: nothing parses docs/ARCHITECTURE.md §7');
}

/**
 * Werdykt nad zrzutem. Czysta funkcja: te same trzy argumenty dają ten sam wynik.
 *
 * @param {{widths: Array<{width: number, metrics: Record<string, number>}>,
 *          notMeasured?: Record<string, string>}} snapshot
 * @param {Array<{key: string, label: string, limit: number}>} ceiling
 * @param {Record<string, number>} baseline zapadka: ostatnio zmierzona wartość per metryka
 * @returns {{verdict: string, over: Array<{metric: string, measured: number, limit: number}>,
 *            notMeasured: string[]}}
 */
export function judge(snapshot, ceiling, baseline) {
  void snapshot;
  void ceiling;
  void baseline;
  throw new Error('judge is not implemented yet: no snapshot has ever been weighed');
}
