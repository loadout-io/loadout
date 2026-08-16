/* Ekran agenta prowadzi dwoma blokami faktów; transkrypt jest trzeci.
 *
 * Wersja oczywista, płynna i średnia to transkrypt na całą wysokość — bo transkrypt jest
 * tym, co mamy pod ręką. Odpowiada na pytanie „co ten agent gadał", a człowiek otwiera
 * agenta, żeby dowiedzieć się dwóch innych rzeczy: co ten agent DOSTAŁ i co po nim ZOSTAŁO.
 * Stąd kolejność `given → produced → transcript` i stąd to, że pierwsze dwie nie biorą się
 * ze słów agenta.
 *
 * Cicha porażka numer jeden całego zadania: blok „co wyprodukował" karmiony ostatnią
 * wiadomością agenta. Agent pisze „I fixed everything", nie zmieniwszy ani jednego pliku,
 * a interfejs podaje jego deklarację w miejscu, w którym człowiek czyta fakty — `agent said`
 * w rubryce `happened` [00-SYNTHESIS §2.2]. Dlatego `produced` powstaje ze zmian na dysku
 * i z przekazań, a deklaracja agenta ma dokładnie jedno miejsce: transkrypt, jako linia
 * `note` podpisana `agent`.
 *
 * Cicha porażka numer dwa, drobniejsza i częstsza: wiersz zastępczy. poprzedni prototyp renderował
 * `SPEND: not reported` i wiersz z niczym w środku wyglądał dokładnie tak samo jak wiersz
 * z liczbą. Wiersz, który nie ma wartości, po prostu nie istnieje; sekcja bez wierszy mówi
 * to jednym zdaniem po angielsku.
 *
 * Czego tu NIE ma i nie ma być: pola do rozmowy z jednym agentem (odłożone, T2 §8.3 §10 —
 * kontrolka bez handlera nie wchodzi do repo, niezmiennik 16) i otwierania panelu zmian
 * (osobna powierzchnia; blok wystawia `detailId` i na tym kończy się jego rola).
 */
import type { FeedView } from '../feed/model';
import type { TranscriptLine } from './filter';

/** Trzy sekcje, w tej kolejności, zawsze. */
export type SectionId = 'given' | 'produced' | 'transcript';

/** Rodzaje wierszy w „co dostał". Zamknięte — piąty rodzaj to nowe kryterium, nie dopisek. */
export type GivenKind = 'step' | 'handoff' | 'note' | 'files';

/** Rodzaje wierszy w „co wyprodukował". Oba są faktami z dysku, nie deklaracjami. */
export type ProducedKind = 'changes' | 'handoff';

export type RowKind = GivenKind | ProducedKind;

/** Jeden wiersz bloku faktów. `value` nigdy nie jest puste ani zastępcze. */
export interface SectionRow {
  readonly kind: RowKind;
  /** Etykieta po angielsku, wielkimi literami w CSS-ie (`STEP`, `FROM ORION`). */
  readonly label: string;
  readonly value: string;
  /** Numer dla panelu szczegółów; sam panel jest osobną powierzchnią. */
  readonly detailId: number | null;
}

export interface Section {
  readonly id: SectionId;
  /** `What <Name> was given` / `What <Name> produced` / `What <Name> said` [makieta 449–467]. */
  readonly heading: string;
  /** Wiersze faktów. Puste dla `transcript`. */
  readonly rows: readonly SectionRow[];
  /** Wiersze strumienia. Niepuste tylko dla `transcript`. */
  readonly lines: readonly TranscriptLine[];
  /** Zdanie po angielsku, gdy sekcja nie ma czego pokazać; `null`, gdy ma. */
  readonly empty: string | null;
}

/** Tyle o agencie, ile potrzebuje nagłówek: podpis w strumieniu i imię na ekranie. */
export interface SessionAgent {
  readonly id: string;
  readonly name: string;
}

/** Krok, na którym stoi agent: co ma zrobić i na jakie pliki mu wskazano. */
export interface StepBrief {
  readonly agent: string;
  readonly name: string;
  readonly brief: string;
  /** Puste, kiedy krok nie wskazał żadnych — wtedy wiersza `files` po prostu nie ma. */
  readonly files: readonly string[];
}

/** Przekazanie: plik, który jeden agent zostawił, a drugi dostał [ARCHITECTURE §8]. */
export interface Handoff {
  readonly from: string;
  readonly to: string;
  readonly file: string;
  readonly summary: string;
  readonly detailId: number | null;
}

/** Zmieniona ścieżka. Fakt z dysku — to jest cała różnica wobec `agent said`. */
export interface Change {
  readonly agent: string;
  readonly path: string;
  readonly added: number;
  readonly removed: number;
  readonly detailId: number | null;
}

/** Notatka „w użyciu", którą Loadout wstrzyknął do promptu tego kroku. */
export interface UsedNote {
  readonly agent: string;
  readonly text: string;
  readonly detailId: number | null;
}

/**
 * Wszystko, z czego powstają trzy sekcje.
 *
 * `view` jest tu tym samym obiektem, który rysuje strumień główny — trzecia sekcja jest
 * jego filtrem, nie jego kopią. Reszta pól to fakty spoza strumienia i żaden z nich nie
 * pochodzi z tego, co agent o sobie powiedział.
 */
export interface SessionInput {
  readonly view: FeedView;
  readonly steps: readonly StepBrief[];
  readonly handoffs: readonly Handoff[];
  readonly changes: readonly Change[];
  readonly notes: readonly UsedNote[];
}

/** Trzy sekcje ekranu agenta, w kolejności `given`, `produced`, `transcript`. */
export function sessionSections(_agent: SessionAgent, _run: SessionInput): readonly Section[] {
  throw new Error('not implemented');
}
