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
import { SECTIONS } from './sections';

/** Ekran sekcji: komponent bez propsów. Powłoka woła go jako `<Screen />`. */
export type Screen = ComponentType;

/** id sekcji → ekran. Sekcja bez ekranu po prostu nie ma tu klucza i to jest cała odpowiedź. */
export type ScreenMap = Partial<Record<Section, Screen>>;

/* Ścieżka, która jest ekranem sekcji: katalog `sections/<id>/` i w nim `index.tsx`. Nic głębiej
 * (`sections/run/rail/panel.tsx` jest częścią ekranu, nie ekranem) i nic płycej.
 *
 * Prefiks jest luźny — `(?:^|/)` zamiast dosłownego `../` — bo klucze, które oddaje odkrywanie,
 * są formatem cudzego narzędzia: dziś wychodzą względne wobec tego pliku, a to jest szczegół
 * implementacji, którego nie kontrolujemy. Kotwiczenie na ogonie ścieżki odpowiada na to samo
 * pytanie i nie zamienia zmiany formatu kluczy w cichą, pustą mapę. */
const SCREEN_PATH = /(?:^|\/)sections\/([^/]+)\/index\.tsx$/;

/* Identyfikatory sekcji jako zbiór — liczony RAZ, przy wczytaniu modułu, a nie w każdym
 * przebiegu pętli. Zbiór jest budowany z `SECTIONS`, więc katalog dopisany do rejestru jest
 * odkrywany bez dotykania tego pliku. */
const KNOWN = new Set<string>(SECTIONS.map((entry) => entry.id));

/** Czy ten katalog jest sekcją, którą ta aplikacja ma. */
function isSection(id: string): id is Section {
  return KNOWN.has(id);
}

/**
 * Czy tym da się coś narysować. JEDNA definicja na całe repo: powłoka pyta o to samo o wpis,
 * który dostała propsem, a odkrywanie o domyślny eksport z dysku (niezmiennik 23). Dwa różne
 * pojęcia „ekranu" znaczyłyby, że mapa przyjmuje wartość, na której powłoka potem pada.
 *
 * Świadomie wąsko: TYLKO funkcja. `memo()` i `forwardRef()` oddają obiekt i wypadłyby tutaj —
 * ale wypadłyby GŁOŚNO, bo kryterium 4 porównuje odkryty zbiór z tym, co leży na dysku, więc
 * taki plik rozjeżdża oba zbiory i świeci na czerwono. Szersze `typeof === 'object'` przepuściłoby
 * za to `{ default: 42 }` prosto do drzewa, czyli dokładnie tę awarię, przed którą to stoi.
 */
export function isScreen(value: unknown): value is Screen {
  return typeof value === 'function';
}

/* Domyślny eksport modułu, albo `undefined`, jeśli moduł nie jest nawet obiektem.
 *
 * Nazwa parametru NIE brzmi `module`: `@types/node` deklaruje `module` w zasięgu globalnym,
 * a parametr, który go przesłania, czyta się jak odwołanie do tamtego i pierwszy czytelnik
 * traci na tym minutę. */
function defaultExport(loaded: unknown): unknown {
  if (typeof loaded !== 'object' || loaded === null) return undefined;
  return (loaded as { default?: unknown }).default;
}

/**
 * Mapa `id → ekran` z surowego wyniku odkrywania: klucz to ścieżka pliku, wartość to moduł.
 * Czysta — nie czyta dysku, więc da się jej podać ręcznie zbudowany rekord.
 */
export function screensFrom(modules: Record<string, unknown>): ScreenMap {
  const found: ScreenMap = {};
  /* Po posortowanych kluczach, nie po kolejności wstawienia. Kolejność, w jakiej przychodzą
   * ścieżki, jest kolejnością, w jakiej system plików wylistował katalogi tego dnia; gdyby
   * dwie ścieżki wskazywały ten sam identyfikator, wynik zależałby od pogody. */
  for (const path of Object.keys(modules).sort()) {
    const id = SCREEN_PATH.exec(path)?.[1];
    if (id === undefined || !isSection(id)) continue;
    const screen = defaultExport(modules[path]);
    if (!isScreen(screen)) continue;
    found[id] = screen;
  }
  return found;
}

/** Ekrany, które naprawdę leżą w `src/sections/`. Jedyne miejsce ze wzorcem ścieżek. */
export function discoverScreens(): ScreenMap {
  return screensFrom(import.meta.glob('../sections/*/index.tsx', { eager: true }));
}
