/* Lokalny worker, który czyta zatwierdzony dokument: tekst ze stronami i wygląd stron.
 *
 * # Worker jedzie w aplikacji, nigdy z sieci
 *
 * `import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url'` każe Vite wyemitować plik
 * workera do `dist/` i oddać jego adres — czyli zwykły import, nie zmiana konfiguracji budowania.
 * Adres jest wtedy tego samego pochodzenia co aplikacja, więc mieści się w `worker-src`, które
 * w `tauri.conf.json` spada do `default-src 'self'`. Aplikacja ma działać bez sieci; worker
 * pobierany z CDN-u byłby awarią tego etapu, nie szczegółem pakowania.
 *
 * # Czego ten plik NIE uruchamia
 *
 * Osadzonych skryptów (`enableScripting` zostaje wyłączone — to jest wartość domyślna i nie
 * włączamy jej), załączników i odnośników: strony renderujemy z adnotacjami wyłączonymi, więc
 * na kanwie nie ląduje ani jedno pole formularza ani jeden odnośnik. `eval` odcina sama karta:
 * CSP aplikacji nie ma `unsafe-eval`, więc szybka ścieżka pdf.js nie ma jak się odpalić
 * niezależnie od tego, o co ten kod poprosi.
 *
 * # Jedna strona naraz
 *
 * Kanwa strony ma rozmiar tej strony w pikselach, więc trzy strony naraz to trzy takie bufory.
 * Każda jest zwalniana zaraz po zakodowaniu (`page.cleanup()` plus wyzerowanie kanwy), a wynik
 * jedzie do Rusta natychmiast — postęp jest ciągiem trwałych zatwierdzeń, nie liczbą w pamięci
 * okna, więc zamknięcie okna w połowie zostawia gotowe strony na dysku.
 *
 * # Czego ta droga nie przygotuje w pełni, i to jest zgłoszone, nie ukryte
 *
 * Obrazy JPEG 2000 (dekoder `wasm`) oraz nieosadzone fonty CJK (katalogi `cmaps/`,
 * `standard_fonts/`) potrzebują albo rozluźnienia CSP, albo dołożenia zasobów do bundla.
 * Jedno i drugie jest decyzją o bezpieczeństwie i pakowaniu, więc taki plik dostaje tu widoczną
 * uwagę i nazwany stan, a nie udawany pusty dokument (AGENTS.md §7).
 */
import * as pdf from 'pdfjs-dist';
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';

import type { Made, Opened, PastedBytes, Unopenable } from '../../state/context';

pdf.GlobalWorkerOptions.workerSrc = workerUrl;

/** Otwiera zatwierdzone bajty i oddaje dokument, który umie oddać stronę po stronie. */
export async function openTheDocument(file: { mime: string; base64: string }): Promise<Opened> {
  const task = pdf.getDocument({
    data: bytesOf(file.base64),
    /* Bez dociągania po kawałku: bajty są już w pamięci, a ta opcja dotyczy pobierania po
     * sieci — czyli drogi, której tu nie ma i mieć nie ma. */
    disableAutoFetch: true,
  });
  let document: pdf.PDFDocumentProxy;
  try {
    document = await task.promise;
  } catch (error) {
    /* WARTOŚĆ, NIE WYJĄTEK. „Nie da się tego otworzyć" jest faktem o pliku, który ma zostać
       zapisany na dysku — rzucony wyjątek kończy się zdaniem znikającym z ekranem. */
    void task.destroy();
    return { failed: whatIsWrong(error) };
  }
  return {
    opened: {
      pages: document.numPages,
      page: async (number: number) => onePage(document, number),
      close: () => {
        void task.destroy();
      },
    },
  };
}

/**
 * Czym plik zawinił — po nazwie błędu, którą podaje `pdf.js`.
 *
 * Hasło i uszkodzenie są tu rozdzielone, bo naprawia się je inaczej: jedno kopią zapisaną bez
 * hasła, drugie ponownym wyeksportowaniem pliku. Wszystko, czego nie umiemy nazwać, idzie jako
 * uszkodzenie — nazwana pomyłka jest lepsza niż stan bez zdania.
 */
function whatIsWrong(error: unknown): Unopenable {
  const named = (error as { name?: unknown } | null)?.name;
  return named === 'PasswordException' ? 'locked' : 'damaged';
}

/**
 * Tekst tej strony i jej wygląd. Pamięć strony wraca do przeglądarki, zanim wyjdziemy stąd.
 *
 * WSZYSTKO TRZY MOŻE PAŚĆ OSOBNO, i to jest cały powód, dla którego ta funkcja niczego nie
 * rzuca: dokument potrafi się otworzyć i rozsypać dopiero przy sięganiu po stronę, przy
 * odczycie jej treści albo przy rysowaniu. Rzucone stąd, każde z tych trzech kończyło się
 * zdaniem na ekranie i plikiem, który dalej „czeka na przygotowanie".
 */
async function onePage(document: pdf.PDFDocumentProxy, number: number): Promise<Made> {
  let page: pdf.PDFPageProxy;
  try {
    page = await document.getPage(number);
  } catch (error) {
    return { failed: whatIsWrong(error) };
  }
  try {
    const content = await page.getTextContent();
    const text = content.items
      .map((item) => ('str' in item ? item.str : ''))
      .join(' ')
      .replace(/[ \t]+/g, ' ')
      .trim();
    return { made: { text, image: await drawn(page) } };
  } catch (error) {
    return { failed: whatIsWrong(error) };
  } finally {
    page.cleanup();
  }
}

/**
 * Wygląd strony jako PNG — albo `null`, kiedy karta nie dała kanwy.
 *
 * `toDataURL`, nie `blob:` — `img-src` w CSP aplikacji dopuszcza `data:`, a adresu typu `blob:`
 * nie ma na tej liście, więc podgląd narysowany przez blob byłby pusty w prawdziwym oknie
 * i pełny w przeglądarce testowej.
 */
async function drawn(page: pdf.PDFPageProxy): Promise<PastedBytes | null> {
  const viewport = page.getViewport({ scale: 1 });
  const canvas = window.document.createElement('canvas');
  canvas.width = Math.ceil(viewport.width);
  canvas.height = Math.ceil(viewport.height);
  if (canvas.getContext('2d') === null) return null;
  try {
    /* CZEKAMY na koniec rysowania. Kanwa odczytana wcześniej jest pusta, a pusty obraz strony
     * przechodzi każdą asercję mówiącą „obraz jest" i nie pokazuje niczego człowiekowi. */
    await page.render({
      canvas,
      viewport,
      /* Adnotacje WYŁĄCZONE: bez tego na kanwie lądują odnośniki i pola formularza, czyli
       * dokładnie te części dokumentu, których ten etap nie ma otwierać. */
      annotationMode: 0,
    }).promise;
    return { mime: 'image/png', base64: canvas.toDataURL('image/png').split(',')[1] ?? '' };
  } finally {
    /* Pamięć strony wraca do przeglądarki OD RAZU: kanwa wielkości strony to kilkanaście
     * megabajtów, a dwustustronicowy plik przygotowywany bez tego trzyma je wszystkie. */
    canvas.width = 0;
    canvas.height = 0;
  }
}

/** Bajty z base64. Po kawałku, bo `atob` na całym pliku zostawia jeden wielki napis w pamięci. */
function bytesOf(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let at = 0; at < binary.length; at += 1) {
    bytes[at] = binary.charCodeAt(at);
  }
  return bytes;
}
