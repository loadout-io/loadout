/* „Uruchom TEN workflow" — intencja, którą zapisuje sekcja Workflow, a wykonuje ekran Run.
 *
 * PO CO TO ISTNIEJE, zmierzone 2026-08-18. Zielony przycisk `Run` w edytorze workflow robił
 * dokładnie jedno: `useSectionStore.getState().go('run')`. Ścieżkę otwartego pliku dostawał
 * propsem (`editor.tsx` podawał `path`) i **wyrzucał ją** (`workflows/index.tsx`), więc
 * w całym łańcuchu nie było ani jednego `invoke`. Skutek dla człowieka: klikasz Run, ekran
 * przeskakuje, nic nie startuje i nic tego nie mówi.
 *
 * Drugi defekt, gorszy, wychodził zaraz za tym: ekran Run wybierał domyślnie `choices[0]`
 * z listy posortowanej BAJTOWO, więc po kliknięciu Run na workflow z dwoma krokami stał tam
 * `New workflow 2` (`'-'` 0x2D wypada przed `'.'` 0x2E), a Start odpowiadał „There are no
 * steps yet." o czymś, co przed chwilą miało dwa kroki. Intencja przewożona tym modułem
 * kasuje oba: mówi WPROST, który plik, i nie zostawia wyboru domyślnego zgadywaniu.
 *
 * DLACZEGO OSOBNY MODUŁ, A NIE PROPS ALBO IMPORT WPROST. Sekcja Workflow nie ma prawa znać
 * polityki startu: „co się dzieje po naciśnięciu Run" mieszka w jednym miejscu — w kontrolce
 * startu i w `io.ts` (niezmiennik 23) — bo tam mieszka też zapadka na drugie kliknięcie,
 * limit „ile naraz" i folder z aktywnej karty. Gdyby edytor wołał `start()` sam, byłby drugim
 * miejscem, w którym te cztery decyzje trzeba podjąć, i pierwszym, które by się rozjechało.
 * Propsem to nie pojedzie, bo te dwie sekcje nie stoją w drzewie jednocześnie: powłoka montuje
 * DOKŁADNIE JEDNĄ sekcję (`src/App.tsx`), więc w chwili kliknięcia ekranu Run jeszcze nie ma.
 *
 * DLACZEGO NIE ZUSTAND. Trzy linie stanu bez selektorów i bez renderu — magazyn dałby tu
 * warstwę, której nikt nie czyta. Prenumerata jest, bo ekran Run może już stać zamontowany
 * (człowiek wraca do niego drugi raz), a wtedy sam `useEffect` przy montażu by tego nie zobaczył.
 */
import { useSectionStore } from '../../ui/shell/section-store';

/** Co dokładnie poproszono uruchomić. */
export interface RunRequest {
  /** Nazwa pliku workflow — ta sama, którą sekcja Workflow nazywa jego plik. */
  readonly path: string;
  /**
   * Numer żądania, ściśle rosnący.
   *
   * Bez niego drugie kliknięcie `Run` na TYM SAMYM workflow byłoby nieodróżnialne od pierwszego
   * i ekran Run nie miałby jak zauważyć, że człowiek poprosił znowu.
   */
  readonly nonce: number;
  /** The visible choice frozen before the editor unmounted the Run screen. */
  readonly reflectionEnabled: boolean;
}

let pending: RunRequest | null = null;
let nonce = 0;
let reflectionForNextRequest = true;
const listeners = new Set<() => void>();

/** Preserve the Run-owned choice only while a different screen carries the next request. */
export function rememberReflectionChoice(enabled: boolean): void {
  reflectionForNextRequest = enabled;
}

/** Seed a remounted Run from the pending editor request, otherwise use the product default. */
export function reflectionForRequestedRun(): boolean {
  return pending?.reflectionEnabled ?? true;
}

/**
 * Poproś o uruchomienie tego workflow i przejdź na ekran pracy.
 *
 * Nawigacja jest TUTAJ, a nie u wołającego, z jednego powodu: żądanie bez przejścia na ekran
 * pracy jest żądaniem, którego nikt nie odbierze, dopóki człowiek sam tam nie wejdzie — czyli
 * przyciskiem, który czasem działa z opóźnieniem kilku minut. Jedno wywołanie, jeden skutek.
 */
export function requestRun(path: string): void {
  nonce += 1;
  pending = { path, nonce, reflectionEnabled: reflectionForNextRequest };
  useSectionStore.getState().go('run');
  for (const listener of listeners) listener();
}

/** Żądanie, które czeka na odebranie, albo `null`. */
export function requestedRun(): RunRequest | null {
  return pending;
}

/**
 * Zdejmuje żądanie, żeby nie zostało odebrane drugi raz.
 *
 * Odbiorca woła to, kiedy już zadziałał. Żądanie zostawione w module wystartowałoby bieg
 * ponownie przy każdym powrocie na ekran pracy — i to jest ta klasa błędu, która kosztuje
 * pieniądze, a nie tylko render.
 */
export function takeRequestedRun(): RunRequest | null {
  const taken = pending;
  pending = null;
  return taken;
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToRequests(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
