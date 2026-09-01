import { defineConfig } from 'vite';
import { configDefaults } from 'vitest/config';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

// Port jest stały i wpisany też w tauri.conf.json. Nie wyliczamy go z liczby
// żywych worktree — spreadsheet tak robił i to jest nazwane błędem (raport 06).
// Kiedy harness potrzebuje portu dla worktree, wyprowadza go z cksum(nazwa).
const PORT = 5273;

export default defineConfig({
  plugins: [react(), tailwindcss()],

  // Tauri czyta stderr; bez tego błędy Vite giną.
  clearScreen: false,

  server: {
    port: PORT,
    strictPort: true,
    // JAWNY IPv4, nie `host: false` — zmierzone 2026-08-17 na prawdziwym oknie.
    // `host: false` każe Node'owi związać "localhost", a to na macOS rozwiązuje się do `::1`,
    // czyli WYŁĄCZNIE IPv6 (`lsof` pokazywał `[::1]:5273`). WKWebView pyta o IPv4, połączenie
    // nie dochodzi i okno jest BIAŁE — bez błędu w konsoli Rusta, bez błędu w konsoli okna,
    // bo nie ma czego zalogować: strona nigdy się nie zaczęła ładować.
    // 127.0.0.1 jest równie lokalny (nic nie wychodzi na LAN), tylko jednoznaczny.
    host: '127.0.0.1',
    watch: {
      // Rust przeładowuje się sam przez cargo-watch; podglądanie target/
      // to tysiące zdarzeń na sekundę i zamrożony dev server.
      //
      // `.claude/**` DOSZŁO 2026-08-23, po tym jak zabiło żywy dev server. Narzędzia sesji
      // zakładają `.claude/worktrees/<nazwa>` jako PEŁNY checkout repo i trzymają go obok przez
      // całą pracę. Zmierzone w dzienniku: pojawienie się takiego katalogu kazało Vite
      // przeładować stronę na `.claude/worktrees/…/index.html`, potem na makietę z `docs/`,
      // potem wyczyścić cache po „zmienionym tsconfigu" — i okno straciło IPC („custom protocol
      // failed"), a proces zszedł. Ta sama przyczyna i to samo lekarstwo, co przy `test.exclude`
      // niżej; brakowało tylko drugiej połowy.
      ignored: ['**/src-tauri/**', '**/target/**', '**/.loadout/**', '**/runs/**', '**/.claude/**'],
    },
  },

  test: {
    // ZMIERZONE 2026-08-19. Narzedzia sesji zakladaja `.claude/worktrees/<nazwa>` jako PELNY
    // checkout repo i trzymaja go przez cala prace sesji obok. Domyslne wykluczenia vitesta
    // tego katalogu nie znaja, wiec `full-test` odkrywal tam DRUGA KOPIE calej suity: 38 s
    // wobec 14 s na czystym drzewie, plus `e2e/tests/*.spec.ts` z tamtej kopii, ktore probuja
    // otworzyc aplikacje i padaja na braku serwera. Bramka byla wtedy czerwona na trunku
    // z powodu tego, ze ktos obok pracuje -- i winna byla pierwsza galaz, ktora chciala ladowac.
    //
    // Domyslne wykluczenia sa ROZSZERZANE, nie nadpisywane: podanie `exclude` w vitescie
    // zastepuje cala liste, wiec bez `configDefaults.exclude` zniknelyby `node_modules` i `dist`.
    exclude: [...configDefaults.exclude, '**/.claude/**'],

    // SUFIT NA ROWNOLEGLOSC. Dopisane 2026-08-31, po zmierzeniu -- i to jest naprawa
    // BRAMKI, nie testu.
    //
    // Vitest bierze domyslnie tyle workerow, ile jest rdzeni: tu szesnascie. Kazdy plik
    // w `e2e/` podnosi WLASNY serwer vite i WLASNE chromium (`e2e/harness.ts`, `booted`
    // jest leniwy na plik), wiec pelny bieg startowal do szesnastu przegladarek naraz.
    // Zmierzone w trakcie takiego biegu: `load average 14,3` przy trzynastu zywych
    // procesach chromium, a `t151-agent-folder-choice-round-trips` przekraczal DWADZIESCIA
    // sekund na pojawienie sie panelu. Ten sam plik osobno: 5,7 s.
    //
    // Czyli bramka mierzyla, co jeszcze chodzi na tym Macu. Werdykt zalezny od pogody uczy
    // ignorowac wlasna czerwien -- a to jest dokladnie ta klasa awarii, przed ktora stoi
    // niezmiennik 19 (kod wyjscia to nie dowod).
    //
    // Osiem, nie szesnascie: kazdy worker e2e to przegladarka plus serwer, wiec polowa
    // rdzeni zostaje na to, co one same odpalaja. Nie jest to `fileParallelism: false` --
    // szeregowanie wszystkiego kosztowaloby kilkanascie minut na suicie, ktora dzis
    // konczy sie w dwudziestu sekundach.
    maxWorkers: 8,
    minWorkers: 1,
  },

  build: {
    // WKWebView na macOS. Windows dostanie swój target, kiedy Windows będzie w planie.
    target: 'safari18',
    sourcemap: true,
    outDir: 'dist',
  },
});
