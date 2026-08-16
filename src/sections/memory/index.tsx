/* Ekran sekcji Memory — SZKIELET FAZY KONTRAKTU, jeszcze bez ciała.
 *
 * Powód pustego ciała stoi w `src/sections/workflows/index.tsx`: szkielet ma się wczytać
 * i paść w czasie wykonania, `throw` jest odpowiednikiem `todo!()` (AGENTS.md §2a), a pusty
 * `<div/>` przepuszcza słabą asercję, którą kryterium ma łapać.
 *
 * CO SKŁADA FAZA WYKONAWCZA. Nagłówek z podpisem liczbowym i DWIE STREFY: „czeka na ciebie"
 * i „w użyciu". Rozdział stref jest tu całym produktem — notatka zaproponowana nie wchodzi
 * do promptu, dopóki człowiek jej nie promuje (T-17), a jedna płaska lista kasuje jedyną
 * widoczną różnicę między tym, co zaproponował agent, a tym, co zatwierdził człowiek.
 * Wiersz notatki (`note-row.tsx`) i okno wymuszonego wyboru (`forced-choice.tsx`) są
 * wylądowane (T-17) i mają własne kryteria — drugiego wiersza nie piszemy (niezmiennik 23).
 *
 * ZGŁOSZENIE DLA CZŁOWIEKA, ZMIERZONE 2026-08-16. Kryterium prosi, żeby notatka zaproponowana
 * niosła „swoje DWIE akcje" (makieta: `Use it` i `Discard`, `docs/mockup/index.html:757`).
 * `NoteRow` renderuje dokładnie JEDNĄ akcję — `Use this` przy `suggested`, `Stop using` przy
 * `in-use` — i tak zamraża to kryterium 6 z T-17. Drugiej akcji nie ma czym obsłużyć:
 * `MemoryState` zna `use`, `stopUsing` i `cancel`, i ani jednego odrzucenia kandydatki.
 * Przycisk `Discard` bez takiej akcji byłby kontrolką bez handlera (niezmiennik 16) —
 * dokładnie tym, co to zadanie cytuje jako powód swojego istnienia. Domknięcie wymaga
 * `discard` w `src/state/memory.ts` i drugiego przycisku w `src/sections/memory/note-row.tsx`;
 * oba pliki są poza blokiem OWNS tego zadania (AGENTS.md §7).
 *
 * O migawce serwerowej zustanda przeczytaj w `src/sections/workflows/index.tsx`.
 */
import type { ReactElement } from 'react';
import { useMemory } from '../../state/memory';

/** Magazyn notatek. Jest singletonem — `src/state/memory.ts` nie ma fabryki. */
export type MemoryStore = typeof useMemory;

export interface MemoryScreenProps {
  /** Bez propsu ekran bierze swój prawdziwy magazyn, z propsem ten z testu. */
  store?: MemoryStore;
}

export default function MemoryScreen(_props: MemoryScreenProps): ReactElement {
  throw new Error('not implemented: keep what waits for you apart from what is in use');
}
