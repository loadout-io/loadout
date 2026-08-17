import { defineConfig } from 'vite';
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
      ignored: ['**/src-tauri/**', '**/target/**', '**/.loadout/**', '**/runs/**'],
    },
  },

  build: {
    // WKWebView na macOS. Windows dostanie swój target, kiedy Windows będzie w planie.
    target: 'safari18',
    sourcemap: true,
    outDir: 'dist',
  },
});
