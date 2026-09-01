/* Co ten workflow ma za sobą — jedna czysta funkcja, żadnego dotknięcia granicy.
 *
 * 2026-08-31 — DLACZEGO TO POWSTAŁO. Kafelek listy mówił dotąd wyłącznie `3 steps · 3 agents`,
 * czyli dwie liczby, które da się policzyć z samego pliku. Nagłówek `tile.tsx` obiecywał od
 * 2026-08-17, że `used 12×` z makiety (`docs/mockup/index.html:642-644`) wchodzi „razem
 * z historią biegów, nigdy jako `—`, `never` ani `not reported`". Historia biegów JEST już na
 * drucie (`list_runs`, `commands::history::RunWire`), więc obietnica jest wykonalna — i to
 * jest cały powód istnienia tego pliku.
 *
 * PO CZYM ŁĄCZYMY BIEG Z PLIKIEM, i to jest ograniczenie drutu, nie wybór wygody.
 * `RunWire` niesie `title`, czyli „jak workflow nazywa SAM SIEBIE" w chwili, gdy ruszał —
 * i ani jednego pola, które wskazywałoby plik. Pole `workflow_file` (nazwa DZISIEJSZEGO pliku,
 * szukana po identyfikatorze z `run.json`) istnieje wyłącznie w `PastRunWire`, czyli w JEDNYM
 * otwartym biegu z `read_run`. Lista, która chciałaby łączyć po pliku, musiałaby otworzyć
 * każdy bieg z osobna — kilkadziesiąt wywołań na wejście do sekcji, po to, żeby narysować
 * jedną linijkę na karcie.
 *
 * Co z tego wynika i co uczciwie mówię zamiast to ukrywać: workflow PRZEMIANOWANY gubi swoją
 * historię (biegi zostają pod starym tytułem), a dwa workflow o tej samej nazwie widzą tę samą.
 * Kafelek nie zmyśla wtedy niczego — pokazuje mniej, nigdy więcej. Domknięcie należy do drutu:
 * `list_runs` musi oddać `workflowFile` tak samo, jak robi to `read_run`, i wtedy ta funkcja
 * zmienia klucz w jednym miejscu.
 */
import type { PastRunRow } from '../../run/io';

/** Wszystko, co lista wie o biegach JEDNEGO workflow. */
export interface RunsBehindIt {
  /** Ile razy ten workflow ruszał. Zawsze co najmniej 1 — zero nie ma tu wpisu. */
  readonly howOften: number;
  /**
   * Najświeższy z nich.
   *
   * Wiersz w całości, a nie przepisane pola: data i słowo stanu mają jedno miejsce
   * zamieszkania (niezmiennik 13), a ten, kto rysuje, tłumaczy je tą samą funkcją,
   * co ekran historii (`sections/run/history-command.ts`).
   */
  readonly latest: PastRunRow;
}

/**
 * Biegi ułożone pod nazwą workflow, którą niosą.
 *
 * `when` jest napisem `YYYY-MM-DD HH:MM` (Rust składa go z nazwy katalogu, UTC), więc
 * porównanie napisów JEST porównaniem chwil — dopóki format ma stałą szerokość. Data
 * parsowana tutaj byłaby drugim czytelnikiem tego samego napisu i pierwszym, który
 * przesunąłby strefę.
 */
export function runsBehindThem(rows: readonly PastRunRow[]): ReadonlyMap<string, RunsBehindIt> {
  const behind = new Map<string, RunsBehindIt>();
  for (const row of rows) {
    /* Bieg, którego opisu Rust nie dał rady przeczytać, ma pusty `title` — a pusta nazwa nie
     * wskazuje żadnego workflow. Doliczony do czegokolwiek byłby liczbą wziętą z niewiedzy. */
    if (row.title.trim() === '') continue;
    const standing = behind.get(row.title);
    if (standing === undefined) {
      behind.set(row.title, { howOften: 1, latest: row });
      continue;
    }
    behind.set(row.title, {
      howOften: standing.howOften + 1,
      latest: row.when > standing.latest.when ? row : standing.latest,
    });
  }
  return behind;
}

/**
 * Który workflow człowiek uruchamiał NAJPÓŹNIEJ — jego nazwa, albo `null`.
 *
 * Odpowiedź na pytanie „od czego zacząć", i to jest jedyny powód, dla którego ta funkcja
 * istnieje: ekran, który daje pierwsze miejsce pozycji ALFABETYCZNIE pierwszej, daje je
 * przypadkowi.
 */
export function lastOneRun(behind: ReadonlyMap<string, RunsBehindIt>): string | null {
  let name: string | null = null;
  let when = '';
  for (const [title, runs] of behind) {
    if (runs.latest.when > when) {
      when = runs.latest.when;
      name = title;
    }
  }
  return name;
}
