/* Które wiersze z drutu mówią „uruchom to teraz", a nie „proponuję to".
 *
 * PO CO TO ISTNIEJE. Rozstrzygnięcie właściciela 2026-08-30: na pytanie „po rozmowie z liderem —
 * klikasz przycisk, czy bieg rusza sam?" odpowiedź brzmiała **„rusza samo"**. Lider, który
 * wywołał czasownik `start_workflow`, kładzie na strumień wiersz z `auto: true`; ten moduł
 * rozstrzyga, że to jest właśnie taki wiersz, i oddaje komendę do uruchomienia.
 *
 * CZEGO TU NIE MA I MIEĆ NIE MOŻE: rozpoznawania komendy w prozie. Czy zdanie lidera jest
 * propozycją, rozstrzyga Rust (`engine::line::suggested`, niezmiennik 15), a czy ma ruszyć samo —
 * pole, które ustawia wyłącznie wywołanie czasownika. Okno, które samo szuka `/run` w akapicie
 * i odpala je bez pytania, jest dokładnie tą awarią, którą właściciel odrzucił w sierpniu:
 * „jak piszę bez komendy… to się na nowo całe workflow odpala".
 *
 * DLACZEGO CZYSTA FUNKCJA, A NIE WARUNEK W `io.ts`. Bo to jest polityka — „co uruchamia bieg" —
 * a to repo nie ma jsdom, więc kodu zamkniętego w uchwycie kanału nie dotknie żadne kryterium.
 * Ta sama rodzina, z której wzięło się siedemnaście kłamiących kontrolek w repo źródłowym.
 */

/** Wiersz z drutu, o który ta funkcja pyta. Wszystko `unknown`, bo tyle właśnie o nim wiadomo. */
interface Row {
  readonly kind: unknown;
  readonly auto: unknown;
  readonly command: unknown;
  readonly agent: unknown;
}

/** Co uruchomić i **kto o to poprosił**. */
export interface Starting {
  /** Komenda znak w znak taka, jaką złożył Rust. */
  readonly command: string;
  /**
   * Podpis, pod którym ma stanąć ewentualna odmowa.
   *
   * Z WIERSZA, nie ze stałej: „jak nazywa się lider" ma jeden dom i jest nim definicja agenta
   * po tamtej stronie (`commands::chat::LEAD`). Nazwa wpisana tutaj byłaby drugą odpowiedzią,
   * która rozjedzie się w dniu, w którym tamta się zmieni — a rozjazd widać wyłącznie jako
   * odmowę podpisaną kimś, kogo nie ma w rozmowie.
   */
  readonly agent: string;
}

/**
 * Komendy do uruchomienia z tej paczki wierszy, w kolejności, w której przyszły.
 *
 * Bierze `unknown[]`, bo dokładnie to przychodzi z kanału — walidacja kształtu jest tutaj, a nie
 * u wołającego. Wiersz, który nie pasuje, jest **porzucany w ciszy**: paczka z drutu nie ma prawa
 * wywrócić okna (niezmiennik 5 w duchu, po stronie frontu), a wiersz bez kompletu pól i tak nie
 * przeszedłby lustra w `src/ipc/types.ts`.
 *
 * TRZY WARUNKI, KAŻDY OSOBNO KONIECZNY. Rodzaj musi być `suggested`, bo tylko ten wiersz niesie
 * komendę; `auto` musi być dokładnie `true`, bo to jedyna rzecz odróżniająca decyzję lidera od
 * jego zdania o komendzie; a komenda musi być niepustym napisem, bo pusta uruchomiłaby pierwszy
 * workflow z brzegu — czyli nie ten, o którym była mowa.
 */
export function autoStarts(batch: readonly unknown[]): readonly Starting[] {
  const going: Starting[] = [];
  for (const line of batch) {
    if (typeof line !== 'object' || line === null) continue;
    const row = line as Row;
    if (row.kind !== 'suggested') continue;
    if (row.auto !== true) continue;
    if (typeof row.command !== 'string' || row.command.trim() === '') continue;
    if (typeof row.agent !== 'string' || row.agent === '') continue;
    going.push({ command: row.command, agent: row.agent });
  }
  return going;
}
