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
 * ── 2026-08-17, implementacja ────────────────────────────────────────────────────────────
 *
 * KONFIGURACJI NIE PRZEPISUJEMY. `createServer` dostaje sam `root`, więc vite czyta
 * `vite.config.ts` z korzenia repo — ten sam plik, z którego powstaje aplikacja. Nadpisany
 * jest DOKŁADNIE jeden klucz, `server`, i to z powodu opisanego akapit wyżej. Wtyczki wpisane
 * tutaj drugi raz zamieniłyby ten harness w drugą definicję builda (niezmiennik 23), a wtedy
 * AC-2 przestałby odpowiadać na swoje własne pytanie: `import.meta.glob` zachowuje się
 * inaczej pod inną konfiguracją i to jest właśnie ta klasa rozjazdu, którą kryterium mierzy.
 *
 * CO ODPOWIADA ATRAPA — i dlaczego to nie jest Rust napisany w teście. Odpowiada, bo musi:
 * front, który dostał `undefined` tam, gdzie spodziewa się listy, przewraca się na własnym
 * `for` i wtedy mierzymy harness, nie produkt. Odpowiada jednak WYŁĄCZNIE KSZTAŁTEM, w trzech
 * linijkach `answer()` niżej: pusta lista dla `list_*`, świeży identyfikator dla `new_id`,
 * `null` dla całej reszty. Ani jedno pole nie jest zmyślone, ani jeden plik nie jest udawany,
 * a `save_workflow` nie oddaje niczego, co ekran mógłby wziąć za zapisany plik — bo to jest
 * pytanie z drugiej strony granicy i odpowiada na nie T-27. Dzień, w którym kryterium tego
 * pliku zacznie potrzebować PRAWDZIWEJ odpowiedzi Rusta, jest dniem, w którym to kryterium
 * należy do T-27, a nie dniem na dopisanie tu czwartej linijki.
 *
 * `transformCallback` i `unregisterCallback` są w atrapie, choć dziś nikt ich nie woła:
 * konstruktor `Channel` z `@tauri-apps/api/core` woła pierwszą z nich BEZ pytania
 * (`core.js:82`), więc bez niej pierwszy przycisk `Start`, który wyląduje, rzuciłby wyjątek
 * i kryterium 3 nazwałoby go martwym. Przycisk ma być sądzony za swój kod, nie za brakującą
 * linijkę w przyrządzie pomiarowym (niezmiennik 20).
 */
import { fileURLToPath } from 'node:url';

import { chromium } from '@playwright/test';
import type { Browser, Page } from '@playwright/test';
import { createServer } from 'vite';
import type { ViteDevServer } from 'vite';

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

/** Korzeń repo: ten plik leży w `e2e/`, więc katalog wyżej. Stąd vite bierze swoją konfigurację. */
const ROOT = fileURLToPath(new URL('..', import.meta.url));

/**
 * Nazwa taśmy w oknie, napisana RAZ i przekazywana obu stronom jako argument.
 *
 * Nagrywanie i odczyt biegną w przeglądarce, czyli w funkcjach, które Playwright serializuje
 * do stringa — takie funkcje nie widzą zasięgu modułu. Literał przepisany w dwóch miejscach
 * rozjechałby się po cichu i dałby „żaden przycisk nic nie woła" o harnessie, nie o produkcie.
 */
const TAPE = '__LOADOUT_E2E_CALLS__';

/** Powłoka w dokumencie. `main[data-section]` renderuje `src/App.tsx` i nic innego. */
const SHELL = 'main[data-section]';

/**
 * Ile czekamy na PIERWSZĄ powłokę.
 *
 * Hojnie, i to nie jest zapas na wszelki wypadek: pierwsze wejście na stronę każe vite
 * przemielić zależności (`node_modules/.vite`), co na zimnym cache'u trwa sekundy, a na
 * ciepłym jest niewidoczne. Limit, który by je ucinał, dawałby kryterium czerwone od pogody
 * maszyny — czyli dokładnie ten rodzaj czerwieni, który uczy ignorować bramkę.
 */
const READY = 30_000;

/** Serwer, przeglądarka i adres — jedna sztuka na plik specyfikacji. */
interface Booted {
  readonly server: ViteDevServer;
  readonly browser: Browser;
  readonly url: string;
}

/* Start jest WSPÓLNY dla całego pliku i leniwy: vite i chromium liczą się w sekundach, a budżet
 * kryterium vitest w bramce to 20 s w `before` i 90 s w `full` (`harness/gate.py`,
 * `CHECK_TIMEOUT`). Świeża jest KARTA, bo to ona niesie stan aplikacji.
 *
 * Promise, a nie gotowy obiekt: dwa `openApp()` wywołane obok siebie mają dostać JEDEN serwer.
 * Flaga „już startuję" plus czekanie na nią to ta sama rzecz napisana dłużej i z wyścigiem. */
let booted: Promise<Booted> | null = null;

async function boot(): Promise<Booted> {
  const server = await createServer({
    root: ROOT,
    /* Bez `configFile` — vite znajdzie `vite.config.ts` sam, w `root`. Patrz nagłówek. */
    logLevel: 'error',
    clearScreen: false,
    server: {
      /* 0 znaczy „daj wolny", a `strictPort` przestaje mieć o czym mówić. */
      port: 0,
      strictPort: false,
      host: '127.0.0.1',
    },
  });
  await server.listen();

  const url = server.resolvedUrls?.local[0];
  if (url === undefined) {
    await server.close();
    throw new Error(
      'the vite dev server started but reports no local URL, so there is nothing to open',
    );
  }

  /* Headless. Okno na ekranie kradłoby fokus w każdym biegu bramki, a `:visible` w kryterium 3
   * pyta o układ, nie o to, czy ktoś patrzy — chromium liczy layout tak samo bez okna. */
  const browser = await chromium.launch();
  return { server, browser, url };
}

/**
 * Otwiera aplikację: serwer vite na wolnym porcie, chromium, atrapa `__TAURI_INTERNALS__`
 * zasiana PRZED pierwszym skryptem strony, i karta z powłoką już w dokumencie.
 */
export async function openApp(): Promise<RunningApp> {
  booted ??= boot();
  const { browser, url } = await booted;

  /* `newPage`, nie `context.newPage`: Playwright daje każdej takiej karcie WŁASNY kontekst
   * i zamyka go razem z nią. To jest cała izolacja, której kryterium 3 potrzebuje — magazyny
   * zustanda żyją w kontekście strony, więc nowa karta to nowy stan aplikacji. */
  const page = await browser.newPage();

  /* Wyjątek ze strony zbieramy od pierwszej chwili. Bez tego jedyną informacją o aplikacji,
   * która wywaliła się przy montowaniu, byłby limit czasu locatora — czyli zdanie „czegoś nie
   * było" zamiast powodu, dla którego nie było. */
  const crashes: string[] = [];
  page.on('pageerror', (error) => {
    crashes.push(error.message);
  });

  await page.addInitScript(recorder, TAPE);
  /* 2026-08-18 — LIMIT PIERWSZEGO WEJŚCIA. Domyślne 30 s Playwrighta mierzy tu nie produkt,
   * tylko pierwsze uruchomienie dev-serwera: vite pre-bunduje zależności na żądanie, a graf
   * modułów urósł o edytor workflow i płótno. Zmierzone: `page.goto` przekraczał 30 s
   * i przewracał CAŁY plik w `beforeAll`, więc trzynaście asercji o produkcie było
   * pomijanych przez koszt startu narzędzia.
   *
   * To nie jest rozluźnienie kryterium: `READY` niżej — limit na pojawienie się powłoki,
   * czyli JEDYNA rzecz, którą ten harness twierdzi o aplikacji — zostaje bez zmian. */
  await page.goto(url, { timeout: 120_000 });

  try {
    await page.locator(SHELL).waitFor({ state: 'attached', timeout: READY });
  } catch (cause) {
    await page.close();
    throw new Error(
      'the application shell never reached the document: nothing matched ' +
        JSON.stringify(SHELL) +
        ' at ' +
        url +
        '.' +
        (crashes[0] === undefined ? '' : ' The page threw first: ' + crashes[0]),
      { cause },
    );
  }

  return {
    page,
    calls: () => page.evaluate(readTape, TAPE),
    close: async () => {
      await page.close();
    },
  };
}

/**
 * Ubija serwer i przeglądarkę. Woła się z `afterAll` każdej specyfikacji.
 *
 * Bez tego proces vitest nie kończy się nigdy: żywy serwer vite i żywy chromium trzymają pętlę
 * zdarzeń, a bramka melduje wtedy przekroczony budżet — czyli kryterium „wisi" zamiast paść
 * albo przejść.
 */
export async function closeEverything(): Promise<void> {
  const pending = booted;
  booted = null;
  if (pending === null) return;

  /* Start, który się nie udał, jest już opowiedziany wyjątkiem z `openApp` — tutaj zostaje po
   * nim wyłącznie odrzucona obietnica. Sprzątanie, które by na niej rzuciło, zamienia czerwone
   * kryterium w nieobsłużony błąd PO teście i chowa powód, dla którego było czerwone. */
  const live = await pending.catch(() => null);
  if (live === null) return;

  /* Najpierw przeglądarka: karta wisząca na żywym module HMR potrafi obudzić serwer w trakcie
   * jego zamykania, a wtedy `close()` czeka na połączenie, którego nikt już nie odbierze. */
  await live.browser.close().catch(() => undefined);
  await live.server.close().catch(() => undefined);
}

/* ── Poniżej biegnie W PRZEGLĄDARCE ─────────────────────────────────────────────────────────
 *
 * Obie funkcje Playwright serializuje do stringa i wykonuje w kontekście strony, więc nie widzą
 * ani jednego identyfikatora z tego modułu: wszystko, czego potrzebują, przyjeżdża argumentem
 * albo stoi w ich własnym ciele. To nie jest styl, tylko warunek działania.
 */

/** Atrapa granicy Rusta. Zasiana `addInitScript`, czyli przed pierwszym skryptem strony. */
function recorder(tape: string): void {
  const host = globalThis as unknown as Record<string, unknown>;

  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  host[tape] = calls;

  /* Kopia Z CHWILI WYSŁANIA, nie odwołanie. Magazyn, który po `await` dopisuje pole do tego
   * samego obiektu, przepisałby wtedy przeszłość na taśmie — a taśma jest jedynym świadkiem
   * tego, co naprawdę przeszło przez granicę. Przy okazji: to, co tu leży, jest już płaskie,
   * więc `page.evaluate` ma co oddać do Node bez zgadywania. */
  const asSent = (args: unknown): Record<string, unknown> => {
    try {
      return JSON.parse(JSON.stringify(args ?? {})) as Record<string, unknown>;
    } catch {
      return {};
    }
  };

  /* Ile identyfikatorów wybiła ta karta. Rośnie, bo dwa kliknięcia mają dostać dwa różne —
   * jeden powtórzony `id` to dwie pozycje listy, które są jedną (`list/store.ts`, `duplicate`). */
  let minted = 0;

  /* CAŁA wiedza atrapy o Rustcie, trzy linijki. Powód, dla którego są trzy, a nie zero i nie
   * trzydzieści, stoi w nagłówku pliku. */
  const answer = (cmd: string): unknown => {
    if (cmd === 'new_id') {
      minted += 1;
      return 'e2e-id-' + String(minted);
    }
    if (cmd.startsWith('list_')) return [];
    return null;
  };

  /* Identyfikatory dla `transformCallback`. Webview trzyma callback pod `window._<id>` i pod
   * tą nazwą woła go z Rusta; atrapa robi dokładnie to samo, żeby `Channel` miał co posprzątać
   * (`core.js:118`, `cleanupCallback`). */
  let handles = 0;

  host['__TAURI_INTERNALS__'] = {
    invoke: (cmd: string, args?: unknown): Promise<unknown> => {
      calls.push({ cmd, args: asSent(args) });
      return Promise.resolve(answer(cmd));
    },

    transformCallback: (callback: (payload: unknown) => void, once = false): number => {
      handles += 1;
      const id = handles;
      const slot = '_' + String(id);
      Object.defineProperty(host, slot, {
        value: (payload: unknown) => {
          if (once) Reflect.deleteProperty(host, slot);
          callback(payload);
        },
        writable: false,
        configurable: true,
      });
      return id;
    },

    unregisterCallback: (id: number): void => {
      Reflect.deleteProperty(host, '_' + String(id));
    },
  };
}

/** Taśma tej karty, przeczytana ze strony. */
function readTape(tape: string): TauriCall[] {
  const host = globalThis as unknown as Record<string, unknown>;
  const calls = host[tape];
  if (!Array.isArray(calls)) {
    /* GŁOŚNO, a nie pustą tablicą. Brak taśmy znaczy, że atrapa nie zdążyła przed skryptami
     * strony — a pusta tablica zamieniłaby tę awarię przyrządu w zdanie „kliknięcie nie doszło
     * do Rusta", czyli w fałszywe oskarżenie produktu (niezmiennik 20). */
    throw new Error(
      'the __TAURI_INTERNALS__ recorder is not on this page: window.' +
        tape +
        ' is missing, so nothing was watching the boundary and "no call was made" would be a ' +
        'statement about this harness, not about the click.',
    );
  }
  return calls as TauriCall[];
}
