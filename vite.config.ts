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
    host: false,
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
