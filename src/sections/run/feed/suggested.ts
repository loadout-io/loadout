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
 * 2026-08-20 — SZKIELET T-61. Ciała rzucają `not implemented`, więc kryteria padają na
 * ZACHOWANIU, a nie na zbieraniu plików: brakujący moduł daje w vitest „Cannot find module",
 * a to jest podpis z `NOT_A_REAL_RED` (AGENTS.md §2a p. 5). Importu `../run-command` tu jeszcze
 * NIE MA i to jest celowe: import bez wywołania jest albo martwą linią, albo `void` udającym
 * użycie — a kryterium i tak pyta o WYWOŁANIE, nie o obecność napisu w źródle (niezmiennik 20).
 */

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
export function suggestion(_command: string): Suggestion | null {
  throw new Error('not implemented');
}

/**
 * Kliknięcie: uruchamia propozycję TĄ SAMĄ drogą, co Enter w wierszu wejścia.
 *
 * Oddaje zdanie odmowy albo `null`, kiedy bieg poszedł — kształt `startFromLine`, znak w znak,
 * bo to jest ta sama odpowiedź i ma być pokazana w ten sam sposób. Odmowa porzucona po drodze
 * jest gorsza niż brak przycisku: człowiek klika i nie dzieje się nic, o czym da się przeczytać.
 */
export async function runSuggestion(_command: string): Promise<string | null> {
  throw new Error('not implemented');
}
