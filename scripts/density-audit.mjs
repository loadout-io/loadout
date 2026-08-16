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
 * GORSZA z dwóch szerokości okna, albo `undefined`, kiedy żadna nie podała liczby.
 *
 * Wszystkie siedem metryk brzmi „co najwyżej N", więc gorsza znaczy większa. Bierzemy
 * gorszą, a nie lepszą ani średnią, bo pomiar przy 1512 px, który chowa się za pomiarem
 * przy 1100 px, meldowałby „pass" na ekranie, którego nikt nie widział takim, jakim
 * został zmierzony.
 *
 * Zero jest tu wartością, nie brakiem: `typeof m === 'number'` odróżnia zmierzone zero
 * od klucza, którego kolektor w ogóle nie zapisał. Na tym rozróżnieniu stoi całe AC-6.
 */
function worst(widths, key) {
  let value;
  for (const at of widths) {
    const measured = at?.metrics?.[key];
    if (typeof measured !== 'number' || Number.isNaN(measured)) continue;
    if (value === undefined || measured > value) value = measured;
  }
  return value;
}

/**
 * Werdykt nad zrzutem. Czysta funkcja: te same trzy argumenty dają ten sam wynik.
 *
 * Cztery stany świata, bo są to cztery różne rzeczy do zrobienia przez człowieka:
 *
 *   `pass`        zmierzone, pod sufitem, pod zapadką
 *   `over`        powyżej sufitu z ARCHITECTURE §7 — „to nie wejdzie do produktu"
 *   `regressed`   pod sufitem, ale powyżej ostatniego pomiaru — „cofnąłeś się"
 *   `unmeasured`  metryki, której nikt nie zmierzył i nikt nie powiedział dlaczego
 *
 * Sufit i zapadka to DWIE RÓŻNE ODMOWY (niezmiennik 18) i zlanie ich w jedno zdanie
 * kasuje całą wartość zapadki: człowiek szuka wtedy regionu, którego nie ma za dużo.
 * Metryka niezmierzona Z POWODEM nie blokuje — powód ma zostać wypisany przy każdym
 * biegu, ale „osie nawigacji" są osądem człowieka i nigdy nie będą liczbą (AC-6).
 *
 * @param {{widths: Array<{width: number, metrics: Record<string, number>}>,
 *          notMeasured?: Record<string, string>}} snapshot
 * @param {Array<{key: string, label: string, limit: number}>} ceiling
 * @param {Record<string, number>} baseline zapadka: ostatnio zmierzona wartość per metryka
 * @returns {{verdict: string, over: Array<{metric: string, measured: number, limit: number}>,
 *            regressed: Array<{metric: string, measured: number, baseline: number}>,
 *            measured: Record<string, number>,
 *            notMeasured: string[], unexplained: string[], reasons: Record<string, string>}}
 */
export function judge(snapshot, ceiling, baseline) {
  const widths = Array.isArray(snapshot?.widths) ? snapshot.widths : [];
  const stated = snapshot?.notMeasured ?? {};

  const over = [];
  const regressed = [];
  // Liczby, które sędzia i tak policzył. Bez nich wołający musiałby powtórzyć `worst()` u
  // siebie, żeby powiedzieć "chromePixels 80/96" albo zapisać zapadkę — a druga kopia tego
  // wyliczenia to dokładnie ten rozjazd, przed którym stoi całe to zadanie. Klucze tej mapy
  // są dopełnieniem `notMeasured`: metryka jest albo tu, albo tam, nigdy w obu i nigdy w żadnym.
  const measured = {};
  const notMeasured = [];
  const unexplained = [];
  const reasons = {};

  for (const entry of ceiling) {
    const value = worst(widths, entry.key);

    if (value === undefined) {
      notMeasured.push(entry.key);
      const reason = stated[entry.key];
      // Powód liczy się tylko wtedy, gdy jest zdaniem. Pusty string to milczenie zapisane
      // tak, żeby wyglądało jak odpowiedź — czyli dokładnie ta awaria, tylko o warstwę wyżej.
      if (typeof reason === 'string' && reason.trim() !== '') {
        reasons[entry.key] = reason;
      } else {
        unexplained.push(entry.key);
      }
      continue;
    }

    measured[entry.key] = value;

    if (value > entry.limit) {
      over.push({ metric: entry.key, measured: value, limit: entry.limit });
    }

    // Metryka nieobecna w zapadce jest pierwszym pomiarem, a pierwszy pomiar zawsze wolno
    // przyjąć — inaczej nowej metryki nie dałoby się włączyć bez ręcznej edycji pliku.
    const seen = baseline?.[entry.key];
    if (typeof seen === 'number' && value > seen) {
      regressed.push({ metric: entry.key, measured: value, baseline: seen });
    }
  }

  let verdict = 'pass';
  if (over.length > 0) verdict = 'over';
  else if (regressed.length > 0) verdict = 'regressed';
  else if (unexplained.length > 0) verdict = 'unmeasured';

  return { verdict, over, regressed, measured, notMeasured, unexplained, reasons };
}
