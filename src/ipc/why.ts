/* Zdanie, które Rust napisał dla człowieka — wyjęte z tego, czym Tauri odrzuciło wywołanie.
 *
 * PO CO TO ISTNIEJE, zmierzone 2026-08-18. Siedem miejsc produkcyjnych pisało
 * `error instanceof Error ? error.message : ''`, a ten warunek jest **zawsze fałszywy**.
 * Skorupy komend robią `.map_err(|e| e.to_string())` (`src-tauri/src/ipc.rs`), Tauri woła
 * `reject(e)` z tym napisem, a `@tauri-apps/api/core` przekazuje go dalej bez opakowania —
 * więc na front przyjeżdża `string`, nigdy `Error`. Skutkiem nie było brzydkie zdanie:
 * użytkownik czytał „Loadout could not start that run." przy KAŻDEJ możliwej przyczynie,
 * a precyzyjne odmowy, które Rust naprawdę produkuje („no agent saved here has the id …",
 * „There are no steps yet.") nie docierały nigdzie. Siedemnaście nieudanych startów w
 * dzienniku właściciela nie zostawiło po sobie ani jednego powodu.
 *
 * JEDNA FUNKCJA NA CAŁE REPO (niezmiennik 23). Wcześniej ten sam warunek stał w siedmiu
 * plikach, każdy z własnym zdaniem zapasowym — czyli siedem miejsc do poprawienia w dniu,
 * w którym granica zmieni kształt. Adapter jest jeden i to on ma znać kształt drutu.
 *
 * DLACZEGO TRZY KSZTAŁTY, A NIE JEDEN. Komendy w tym repo odrzucają dwiema drogami i obie są
 * poprawne: `Result<_, String>` daje na drucie napis, a `Result<_, NoteRefusal>` daje OBIEKT
 * z polem `message` (`src-tauri/src/commands/memory.rs`). Trzeci kształt, `Error`, zostaje,
 * bo tą drogą przychodzą awarie SAMEGO frontu — zerwany kanał, wyjątek z `@tauri-apps/api` —
 * i one też mają prawo się pokazać. Cokolwiek innego zostaje bez zdania, a wtedy odpowiada
 * wołający: własne zdanie zapasowe jest ostatnią gałęzią, nigdy pierwszą.
 */

/**
 * Co powiedział Rust, albo `mine`, jeśli nie powiedział nic.
 *
 * @param error to, czym odrzuciło `invoke` — napis, `Error`, obiekt z `message`, albo cokolwiek.
 * @param mine zdanie zapasowe TEGO wołającego. Ma mówić, co się nie udało JEMU, a nie
 *   „coś poszło nie tak": zdanie ogólne w miejscu, gdzie znamy czynność, jest gorsze niż brak.
 */
export function why(error: unknown, mine: string): string {
  const said = saidBy(error);
  return said === '' ? mine : said;
}

/**
 * Sam napis z odmowy, bez zdania zapasowego — `''`, kiedy odmowa nic nie powiedziała.
 *
 * Osobno od [`why`], bo dwa wołające potrzebują właśnie tego rozróżnienia: magazyn, który
 * decyduje, czy w ogóle jest o czym mówić, nie może dostać zdania zapasowego wybranego
 * za niego.
 */
export function saidBy(error: unknown): string {
  if (typeof error === 'string') {
    return error.trim();
  }
  if (error instanceof Error) {
    return error.message.trim();
  }
  /* Obiekt z `message`: tą drogą przyjeżdżają odmowy o własnym typie, np. `NoteRefusal`.
   * Sprawdzamy pole, nie klasę — po drugiej stronie granicy nie ma żadnych klas, jest JSON. */
  if (typeof error === 'object' && error !== null) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === 'string') {
      return message.trim();
    }
  }
  return '';
}
