/* Jak powłoka znajduje ekran sekcji: szuka `src/sections/<id>/index.tsx` i nic poza tym.
 *
 * Konwencja zamiast rejestru (HARNESS-QUEUE.md Q-5, decyzja Jakuba 2026-08-15). Powiązanie
 * id → ekran wynika ze ŚCIEŻKI pliku, więc każde zadanie sekcji dokłada swój `index.tsx`
 * w poddrzewie, które już posiada: zero plików dzielonych, zero wpisów do cudzego bloku OWNS,
 * zero konfliktów przy landowaniu. Pole `component` w `src/ui/sections.tsx` zrobiłoby z rejestru
 * drugi wspólny kręgosłup obok `lib.rs`, z dokładnie tą samą klasą kolizji — a front, inaczej
 * niż Rust, niczego takiego nie wymaga.
 *
 * Dwie funkcje, celowo rozdzielone. Wszystko, co da się pomylić, siedzi w czystym `screensFrom`
 * i da się to sprawdzić bez dotykania dysku; `discoverScreens` jest jedną linią bez ani jednego
 * warunku. Wzorzec ścieżek występuje przez to DOKŁADNIE RAZ (niezmiennik 23) — drugi wzorzec,
 * w teście albo w `App.tsx`, byłby drugim miejscem do rozjechania się.
 *
 * Dwie rzeczy są pomijane W CISZY, obie z tego samego powodu: cudzy plik nie ma prawa zabrać
 * całego okna (niezmiennik 5 w duchu, po stronie frontu).
 *   - katalog, którego nazwa nie jest identyfikatorem z `SECTIONS` (`src/sections/quantum/`),
 *   - moduł bez eksportu, który da się wyrenderować.
 * Sekcja pokazuje wtedy swój pusty ekran — czyli traci jedna sekcja, nie aplikacja.
 */
import type { ComponentType } from 'react';
import type { Section } from './sections';

/** Ekran sekcji: komponent bez propsów. Powłoka woła go jako `<Screen />`. */
export type Screen = ComponentType;

/** id sekcji → ekran. Sekcja bez ekranu po prostu nie ma tu klucza i to jest cała odpowiedź. */
export type ScreenMap = Partial<Record<Section, Screen>>;

/**
 * Mapa `id → ekran` z surowego wyniku odkrywania: klucz to ścieżka pliku, wartość to moduł.
 * Czysta — nie czyta dysku, więc da się jej podać ręcznie zbudowany rekord.
 */
export function screensFrom(modules: Record<string, unknown>): ScreenMap {
  /* Szkielet fazy kontraktu: sygnatura już jest, ciała jeszcze nie ma. `void` zamiast `_modules`,
   * żeby nazwa parametru była od razu ta docelowa — implementacja podmienia samo ciało. */
  void modules;
  throw new Error('not implemented');
}

/** Ekrany, które naprawdę leżą w `src/sections/`. Jedyne miejsce ze wzorcem ścieżek. */
export function discoverScreens(): ScreenMap {
  throw new Error('not implemented');
}
