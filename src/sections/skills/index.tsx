/* Ekran sekcji Skills — SZKIELET FAZY KONTRAKTU, jeszcze bez ciała.
 *
 * Powód pustego ciała stoi w `src/sections/workflows/index.tsx`: szkielet ma się wczytać
 * i paść w czasie wykonania, `throw` jest odpowiednikiem `todo!()` (AGENTS.md §2a), a pusty
 * `<div/>` przepuszcza słabą asercję, którą kryterium ma łapać.
 *
 * CO SKŁADA FAZA WYKONAWCZA. Nagłówek z podpisem liczbowym, jedna akcja dodawania (makieta:
 * `＋ Add a skill`, prowadzi do wklejenia linku, czyli do `useSkills.review`) i lista
 * umiejętności, każda ze znacznikiem stanu rozmieszczenia. Karty przeglądu nie piszemy
 * drugi raz: `ReviewCard` jest wylądowany (T-19) i ma własne kryteria (niezmiennik 23).
 *
 * ZGŁOSZENIE DLA CZŁOWIEKA, ZMIERZONE 2026-08-16. Kryterium prosi o „znacznik mówiący, dla
 * ilu vendorów umiejętność jest rozmieszczona, wyliczony ze stanu" przy DWÓCH umiejętnościach
 * o różnym stanie rozmieszczenia. `InstalledSkill` w `src/state/skills.ts` ma dokładnie dwa
 * pola — `name` i `fromTheInternet` — i ani jednego o vendorach, a `src/state/skills.ts` jest
 * poza blokiem OWNS tego zadania. Dwie POZYCJE `installed` nie mają dziś jak różnić się
 * rozmieszczeniem: takiego stanu nie da się nawet zasiać w teście, bo nie ma pola, w którym
 * by mieszkał. Jedyna różnica rozmieszczenia, jaką ten magazyn niesie, to „już leży
 * w katalogach obu vendorów" (`installed`) kontra „jeszcze czeka na człowieka" (`pending`) —
 * i na tym stoi kryterium. Pełne odczytanie wymaga pola per vendor od T-18, czyli zapisu
 * w cudzym pliku (AGENTS.md §7).
 *
 * O migawce serwerowej zustanda przeczytaj w `src/sections/workflows/index.tsx`.
 */
import type { ReactElement } from 'react';
import { useSkills } from '../../state/skills';

/** Magazyn umiejętności. Jest singletonem — `src/state/skills.ts` nie ma fabryki. */
export type SkillsStore = typeof useSkills;

export interface SkillsScreenProps {
  /** Bez propsu ekran bierze swój prawdziwy magazyn, z propsem ten z testu. */
  store?: SkillsStore;
}

export default function SkillsScreen(_props: SkillsScreenProps): ReactElement {
  throw new Error('not implemented: show the skills and who they are ready for');
}
