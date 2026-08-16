/* Sufit gęstości: kolektor i sędzia, rozdzieleni celowo.
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
import { readFileSync } from 'node:fs';

/** Nagłówek sekcji, w której mieszka tabela sufitu. Jedyne źródło siedmiu liczb. */
const SECTION = '## 7. Sufit gęstości';

/*
 * ETYKIETY wierszy i KLUCZE maszynowe — nazwy, nigdy limity.
 *
 * To nie jest druga kopia sufitu, bo nie ma tu ani jednej liczby: liczby przychodzą
 * z dokumentu przy każdym wywołaniu. Nazwy muszą stać tutaj, bo bez nich parser nie ma
 * jak stwierdzić, że wiersza BRAKUJE — umiałby tylko zwrócić sześć wpisów i zamilknąć,
 * a metryka, której nikt nie mierzy, czyta się dokładnie jak metryka, która się mieści.
 * Kolejność jest kolejnością wierszy tabeli §7 i też jest częścią kontraktu: "siódmy
 * wiersz" musi dać się nazwać.
 */
const ROWS = [
  { key: 'labelledRegions', label: 'Oznaczone regiony na ekranie' },
  { key: 'chromePixels', label: 'Piksele chrome nad pierwszą treścią' },
  { key: 'textElements', label: 'Elementy niosące tekst w widoku domyślnym' },
  { key: 'liveRegionsPerFact', label: 'Żywe regiony na jeden fakt' },
  { key: 'agentCardLines', label: 'Linie tekstu w kafelku agenta' },
  { key: 'animatedRegions', label: 'Regiony animujące się od jednego zdarzenia' },
  { key: 'navigationAxes', label: 'Osie nawigacji na ekranie' },
];

/** Komórki wiersza tabeli markdown, bez rurek i bez białych znaków na brzegach. */
function cells(line) {
  let inner = line.trim();
  if (inner.startsWith('|')) inner = inner.slice(1);
  if (inner.endsWith('|')) inner = inner.slice(0, -1);
  return inner.split('|').map((cell) => cell.trim());
}

/**
 * Liczba z komórki limitu.
 *
 * Siódmy wiersz brzmi `**2**, i muszą być prostopadłe` — limit bywa zdaniem, nie samą
 * liczbą, więc bierzemy pogrubioną wartość, a dopiero w drugiej kolejności gołą. Czego
 * NIE robimy: nie zwracamy wartości domyślnej, kiedy liczby nie ma. Domyślna wartość
 * w tym miejscu to dokładnie ta cicha awaria, przed którą stoi całe to zadanie.
 */
function limitFrom(cell) {
  const bold = /\*\*(\d+)\*\*/.exec(cell);
  if (bold?.[1] !== undefined) return Number(bold[1]);
  const bare = /(\d+)/.exec(cell);
  if (bare?.[1] !== undefined) return Number(bare[1]);
  return undefined;
}

/** Wiersze tabeli §7 jako mapa etykieta → komórka limitu. Rzuca, kiedy tabeli nie ma. */
function declaredRows(path) {
  const lines = readFileSync(path, 'utf8').split('\n');

  const start = lines.findIndex((line) => line.trim() === SECTION);
  if (start === -1) {
    throw new Error(
      `density ceiling: ${path} has no section titled "${SECTION}", ` +
        'so the seven limits are declared nowhere this audit can read',
    );
  }

  // Sekcja kończy się na następnym nagłówku drugiego poziomu — nie na `---`, bo pozioma
  // linia stoi też pod tabelą i ucięłaby prozę razem z częścią wierszy.
  let end = lines.length;
  for (let i = start + 1; i < lines.length; i += 1) {
    if (lines[i]?.startsWith('## ') === true) {
      end = i;
      break;
    }
  }
  const section = lines.slice(start + 1, end);

  const first = section.findIndex((line) => line.trimStart().startsWith('|'));
  if (first === -1) {
    throw new Error(`density ceiling: section "${SECTION}" of ${path} holds no table at all`);
  }
  let last = first;
  while (section[last + 1]?.trimStart().startsWith('|') === true) last += 1;

  // Pierwsze dwa wiersze każdej tabeli markdown to nagłówek i separator. Odcinamy je tutaj,
  // żeby niżej KAŻDY pozostały wiersz był wierszem danych i dało się odmówić przy nieznanym.
  const body = section.slice(first + 2, last + 1);
  if (body.length === 0) {
    throw new Error(`density ceiling: the table in section "${SECTION}" of ${path} has no rows`);
  }

  const declared = new Map();
  for (const line of body) {
    const cell = cells(line);
    const label = cell[0];
    if (label === undefined || label === '') continue;
    if (declared.has(label)) {
      throw new Error(
        `density ceiling: ${path} §7 declares the row "${label}" twice, ` +
          'so which of the two limits binds is undecidable',
      );
    }
    declared.set(label, cell[1] ?? '');
  }
  return declared;
}

/**
 * Siedem wpisów sufitu, w kolejności wierszy tabeli §7.
 *
 * @param {string} path ścieżka do docs/ARCHITECTURE.md
 * @returns {Array<{key: string, label: string, limit: number}>}
 */
export function readCeiling(path) {
  const declared = declaredRows(path);

  // Wiersz, o którym ten plik nie wie, jest metryką, której nikt nie zmierzy — a to jest ta
  // sama awaria, co brakujący wiersz, tylko odwrócona. Odmawiamy po nazwie zamiast milczeć.
  const known = new Set(ROWS.map((row) => row.label));
  for (const label of declared.keys()) {
    if (!known.has(label)) {
      throw new Error(
        `density ceiling: §7 of ${path} declares a row this audit does not measure: "${label}"`,
      );
    }
  }

  return ROWS.map(({ key, label }) => {
    const cell = declared.get(label);
    if (cell === undefined) {
      throw new Error(
        `density ceiling: §7 of ${path} has no row named "${label}", ` +
          'and a metric nobody declares reads exactly like a metric that fits',
      );
    }
    const limit = limitFrom(cell);
    if (limit === undefined) {
      throw new Error(
        `density ceiling: the row "${label}" in §7 of ${path} states no number ` +
          `(its limit cell reads "${cell}")`,
      );
    }
    return { key, label, limit };
  });
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
