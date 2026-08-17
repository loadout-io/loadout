/* Harness kryteriów T-29: prawdziwy front, prawdziwa przeglądarka, prawdziwe kliknięcie —
 * i granica postawiona dokładnie tam, gdzie zaczyna się Rust.
 *
 * PO CO TO ISTNIEJE. Sześć razy 2026-08-16 kryterium było zielone, a produkt nie działał:
 * sekcja bez `index.tsx` renderowała pusty ekran, przycisk nie miał handlera,
 * `renderToStaticMarkup` nigdy nie odpala `onClick`. Za każdym razem dowiadywał się o tym
 * człowiek, który uruchomił aplikację. Ten plik daje ten sam dowód bramce: klik przechodzi
 * przez prawdziwy DOM, prawdziwy React, prawdziwy magazyn i prawdziwy adapter.
 *
 * CZEGO TO NIE DOWODZI, i to jest granica, nie kompromis do ukrycia. `tauri-driver` obsługuje
 * Linuksa i Windows; na macOS okno Tauri to WKWebView i nie ma czym nim wysterować. Żaden test
 * w tym repo nie klika po PRAWDZIWYM oknie aplikacji i ten plik tego nie zmienia. Sterujemy
 * warstwą, którą DA SIĘ wysterować: frontem na vite, z `window.__TAURI_INTERNALS__` podstawionym
 * przez nagrywającą atrapę. Drugą stronę tej granicy dowodzi T-27 (`commands.golden.txt` czytana
 * z obu stron plus przebiegi tam-i-z-powrotem przez prawdziwe funkcje). Kryterium, które
 * UDAWAŁOBY, że klika po Tauri, byłoby gorsze niż brak kryterium.
 *
 * DLACZEGO ATRAPA SIEDZI W `__TAURI_INTERNALS__`, A NIE W `@tauri-apps/api/core`. Podmiana
 * modułu (`vi.mock`) działa po stronie testu, a tutaj kod biegnie w PRZEGLĄDARCE — moduł jest
 * już zbudowany przez vite i test nie ma go jak podmienić. Za to `invoke()` z tej paczki czyta
 * `window.__TAURI_INTERNALS__.invoke(cmd, args, options)` dopiero w chwili wywołania
 * (`node_modules/@tauri-apps/api/core.js:202`), więc atrapa zasiana przed pierwszym skryptem
 * strony łapie KAŻDE wywołanie, którejkolwiek drogi by nie użyto. To jest ta sama różnica,
 * co między testem krawędzi a testem produktu: nie pytamy, czy `io.ts` woła `invoke`, tylko
 * czy DOJDZIE tam kliknięcie człowieka.
 *
 * DLACZEGO WOLNY PORT, A NIE 5273 z `vite.config.ts`. Ten port jest stały i wpisany też
 * w `tauri.conf.json`, więc jest ZAJĘTY, kiedy ktoś ma otwartą aplikację albo `npm run dev` —
 * a `strictPort: true` zamienia to w `EADDRINUSE`, który stoi na liście `NOT_A_REAL_RED`
 * w bramce. Kryterium przewrócone przez cudzy otwarty terminal nie mówi nic o kodzie.
 * Równoległe worktree mają ten sam problem i tę samą odpowiedź.
 *
 * 2026-08-17 — SZKIELET. Ciała wypełnia faza implementacji; tutaj są wyłącznie sygnatury,
 * bo trzy specyfikacje obok mają się skompilować i paść na BRAKU ZACHOWANIA, a nie na braku
 * modułu (AGENTS.md §2a). To jest dokładny odpowiednik `todo!()` po stronie Rusta.
 */
import type { Page } from '@playwright/test';

/** Jedno wywołanie, które przeszło przez atrapę `__TAURI_INTERNALS__` w stronę Rusta. */
export interface TauriCall {
  /** Pierwszy argument `invoke` — nazwa komendy, znak w znak jak w `commands.golden.txt`. */
  readonly cmd: string;
  /**
   * Drugi argument `invoke`. Nazwy pól są tym, co napisał front: Tauri przepisuje je dopiero
   * po drugiej stronie, więc porównywanie ich TUTAJ mówiłoby o konwencji, a nie o danych.
   */
  readonly args: Record<string, unknown>;
}

/**
 * Jedna otwarta aplikacja: własna karta, własny magazyn, własna taśma wywołań.
 *
 * „Świeżo otwarta" znaczy tu dosłownie świeża — magazyny zustanda żyją na poziomie modułu,
 * czyli w kontekście strony, więc nowa strona to nowy stan. Ponowne wejście na tę samą sekcję
 * w tej samej karcie NIE jest tym samym i kryterium 3 opiera się właśnie na tej różnicy.
 */
export interface RunningApp {
  /** Karta z załadowaną aplikacją. Powłoka jest już w dokumencie, kiedy to dostajesz. */
  readonly page: Page;

  /**
   * Wszystko, co poleciało do Rusta od otwarcia tej karty, w kolejności wysłania.
   *
   * Czytane ze strony przy każdym wywołaniu, a nie zbierane w Node: taśma mieszka tam, gdzie
   * dzieje się `invoke`, więc nie ma między nią a kliknięciem ani jednego kanału, który mógłby
   * zgubić wywołanie i zamienić martwy przycisk w żywy.
   */
  calls(): Promise<readonly TauriCall[]>;

  /** Zamyka tę kartę. Serwer i przeglądarka zostają dla następnej. */
  close(): Promise<void>;
}

/**
 * Otwiera aplikację: serwer vite na wolnym porcie, chromium, atrapa `__TAURI_INTERNALS__`
 * zasiana PRZED pierwszym skryptem strony, i karta z powłoką już w dokumencie.
 *
 * Serwer i przeglądarka powstają przy pierwszym wywołaniu i są wspólne dla całego pliku —
 * start vite i start chromium liczą się w sekundach, a budżet kryterium vitest w bramce
 * to 20 s w `before` i 90 s w `full` (`harness/gate.py`, `CHECK_TIMEOUT`). Świeża jest KARTA,
 * bo to ona niesie stan aplikacji.
 */
export async function openApp(): Promise<RunningApp> {
  throw new Error(
    'not implemented: e2e/harness.ts is a skeleton — it starts no vite server, no browser ' +
      'and seeds no __TAURI_INTERNALS__ recorder yet',
  );
}

/**
 * Ubija serwer i przeglądarkę. Woła się z `afterAll` każdej specyfikacji.
 *
 * Bez tego proces vitest nie kończy się nigdy: żywy serwer vite i żywy chromium trzymają pętlę
 * zdarzeń, a bramka melduje wtedy przekroczony budżet — czyli kryterium „wisi" zamiast paść
 * albo przejść.
 */
export async function closeEverything(): Promise<void> {
  /* Celowo TOTALNA, i to nie jest zaślepka udająca sukces: `openApp` odmawia, zanim powstanie
   * serwer albo przeglądarka, więc w szkielecie naprawdę nie ma tu czego ubijać. Sprzątanie,
   * które rzuca, zamienia czerwone kryterium w nieobsłużony błąd PO teście i chowa powód,
   * dla którego było czerwone — a powód jest jedyną rzeczą, po którą się tu przychodzi. */
}
