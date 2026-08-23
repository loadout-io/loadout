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
import { sessionFeed } from './filter';

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
  /**
   * O co poproszono ten krok. **Puste znaczy „nie wiemy"** i wtedy wiersz niesie samą nazwę
   * kroku — dziś tak jest zawsze w prawdziwym biegu, bo `planOf` nie przewozi `instructions`
   * z pliku workflow do magazynu biegu. Zgłoszone jako brak na drucie okna.
   */
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
  /**
   * Ile ten plik waży, gotowe do przeczytania (`2.5 KB`). Nieobecne znaczy „nie wiemy".
   *
   * 2026-08-23 — POLE DOSZŁO, BO LICZBA JEST NA DRUCIE. Nagłówek `./session.tsx` wymieniał
   * trzecią kolumnę makiety (`.sz`) wśród rzeczy, których NIE rysujemy, i miał rację w dniu,
   * w którym to pisano: rozmiaru nie było skąd wziąć, a `—` w tej samej siatce i tym samym
   * krojem, co prawdziwa liczba, jest wierszem zastępczym nie do odróżnienia od wiersza
   * z wartością (niezmiennik 17). `HandoffWire::bytes` jest MIERZONE przy odczycie, więc
   * dziś to jest fakt — i jedyna uczciwa odpowiedź na pytanie, czy poprzednik zostawił
   * research, czy dwa zdania. Opcjonalne, bo wołający bez pomiaru ma nie pisać zera.
   */
  readonly size?: string;
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

/**
 * Zdania pustych stanów — jedno na sekcję, po angielsku, bez nazw pól z danych.
 *
 * Puste `given` naprawdę się zdarza: pod-agent rozpuszczony w trakcie biegu nie stoi na
 * żadnym kroku, nikt mu nic nie przekazał i żadna notatka nie poszła do jego promptu. Puste
 * `produced` zdarza się jeszcze częściej i jest tym przypadkiem, dla którego cały ten podział
 * powstał: agent pisze „I fixed everything", nie zmieniwszy ani jednego pliku.
 *
 * Zdanie, nie kreska. `SPEND: not reported` w poprzednim prototypie stało w tej samej siatce i tym
 * samym krojem, co wiersz z prawdziwą liczbą, więc jednego od drugiego nie dało się odróżnić
 * inaczej niż czytając.
 */
const NOTHING: Readonly<Record<SectionId, string>> = {
  given: 'Nothing was given to this agent.',
  produced: 'No files changed and nothing handed on yet.',
  transcript: 'This agent has not said anything yet.',
};

/** Wiersz albo nic. Sekcja nie dostaje wiersza, którego wartość byłaby pusta. */
function row(kind: RowKind, label: string, value: string, detailId: number | null): SectionRow {
  return { kind, label, value, detailId };
}

/**
 * Kilka faktów w jednym wierszu — bez separatora po tym, którego nie ma.
 *
 * `a + ' · ' + b` dla pustego `b` daje wiersz kończący się kropką i spacją: dokładnie ten
 * wiersz zastępczy, przed którym stoi nagłówek tego pliku, tylko bez słowa, które by go
 * zdradziło. Fakt, którego nie mamy, nie zostawia po sobie nawet kreski.
 */
function parts(...values: readonly (string | undefined)[]): string {
  return values.filter((one) => one !== undefined && one !== '').join(' · ');
}

/**
 * Co ten agent dostał: krok, przekazania do niego, notatki w użyciu, wskazane pliki.
 *
 * Kolejność jest kolejnością z makiety (linie 509–517). Wiersza, dla którego nie ma wartości,
 * po prostu nie ma — pięć wierszy wpisanych na stałe za makietą, z których trzy są puste,
 * wygląda na skończone i mówi mniej.
 */
function givenRows(agent: SessionAgent, run: SessionInput): readonly SectionRow[] {
  const steps = run.steps.filter((step) => step.agent === agent.id);
  const rows: SectionRow[] = [];

  for (const step of steps) {
    /* PUSTY BRIEF ZOSTAWIA SAMĄ NAZWĘ KROKU, i to jest naprawa z 2026-08-18, nie wygoda.
     *
     * Prompt kroku (`AgentStep.instructions`) jest czytany z dysku przez sekcję Bieg i GUBIONY
     * w `choices.ts` (`planOf` przepisuje tylko `id`, `name`, `state`), więc magazyn biegu nie
     * ma dziś czym tego briefu przewieźć. `name + ' — ' + ''` dawało wiersz kończący się
     * półpauzą i spacją: dokładnie ten wiersz zastępczy, przed którym stoi nagłówek tego pliku,
     * tylko bez słowa, które by go zdradziło. Nazwa kroku jest faktem i zostaje sama. */
    rows.push(
      row('step', 'Step', step.brief === '' ? step.name : step.name + ' — ' + step.brief, null),
    );
  }
  for (const handoff of run.handoffs) {
    if (handoff.to !== agent.id) continue;
    /* Etykieta niesie nadawcę, bo to jest pierwsza rzecz, o którą się pyta przy przekazaniu
     * („od kogo to przyszło"), a treść wiersza jest wtedy samym plikiem [makieta 511]. */
    rows.push(
      row(
        'handoff',
        'From ' + handoff.from,
        parts(handoff.file, handoff.summary, handoff.size),
        handoff.detailId,
      ),
    );
  }
  for (const note of run.notes) {
    /* PUSTY WŁAŚCICIEL ZNACZY „KAŻDEMU", i to nie jest rozluźnienie filtra. Notatka o zakresie
     * `everywhere` albo `this-project` nie należy do żadnego agenta i wjeżdża w prompt KAŻDEGO
     * kroku (`commands::run::what_this_run_knew`), więc jest faktem o każdym z nich. Odsianie
     * jej tutaj dawałoby blok, który o połowie tego, co model wiedział, milczy — a milczy
     * dokładnie tak samo, jak blok agenta, któremu naprawdę nic nie dano. */
    if (note.agent !== '' && note.agent !== agent.id) continue;
    rows.push(row('note', 'Note', note.text + ' — in use', note.detailId));
  }
  for (const step of steps) {
    /* Krok, który nie wskazał żadnego pliku, nie dostaje wiersza `files` z kreską w środku. */
    if (step.files.length === 0) continue;
    rows.push(row('files', 'Files', step.files.join(', '), null));
  }
  return rows;
}

/**
 * Co po tym agencie zostało: zmienione ścieżki i przekazania od niego.
 *
 * Oba są faktami z dysku i to jest cała teza tego bloku. Karmienie go ostatnią wiadomością
 * agenta jest cichą porażką numer jeden całego ekranu: deklaracja postawiona w rubryce faktów
 * czyta się jak fakt i nie ma na ekranie niczego, co by jej zaprzeczyło [00-SYNTHESIS §2.2].
 * Zmiana zrobiona przez innego agenta należy do TAMTEGO agenta.
 */
function producedRows(agent: SessionAgent, run: SessionInput): readonly SectionRow[] {
  const rows: SectionRow[] = [];

  for (const change of run.changes) {
    if (change.agent !== agent.id) continue;
    rows.push(
      row(
        'changes',
        'Changes',
        change.path + ' · +' + String(change.added) + ' −' + String(change.removed),
        change.detailId,
      ),
    );
  }
  for (const handoff of run.handoffs) {
    if (handoff.from !== agent.id) continue;
    /* „Passed on", nie „Handoff": etykieta stoi na ekranie, a nazwa mechanizmu z dokumentacji
     * nie jest słowem, którym człowiek o tym myśli (niezmiennik 14). Do 2026-08-23 ta linia
     * nie renderowała się ANI RAZU — `mount.tsx` podawał `handoffs: []` na sztywno — więc
     * dzień, w którym blok zaczyna mówić prawdę, jest pierwszym dniem, w którym ta etykieta
     * kogokolwiek obchodzi. */
    rows.push(
      row(
        'handoff',
        'Passed on',
        parts(handoff.file, handoff.summary, handoff.size),
        handoff.detailId,
      ),
    );
  }
  return rows;
}

/** Sekcja faktów: wiersze albo jedno zdanie o tym, że ich nie ma. */
function factsSection(
  id: 'given' | 'produced',
  name: string,
  rows: readonly SectionRow[],
): Section {
  return {
    id,
    heading: 'What ' + name + (id === 'given' ? ' was given' : ' produced'),
    rows,
    lines: [],
    empty: rows.length === 0 ? NOTHING[id] : null,
  };
}

/** Trzy sekcje ekranu agenta, w kolejności `given`, `produced`, `transcript`. */
export function sessionSections(agent: SessionAgent, run: SessionInput): readonly Section[] {
  /* Transkrypt jest trzeci i to jest cała decyzja tego pliku. Wersja oczywista — transkrypt
   * na całą wysokość — odpowiada na pytanie „co ten agent gadał", a człowiek otwiera agenta,
   * żeby dowiedzieć się dwóch innych rzeczy. */
  const lines = sessionFeed(run.view, agent.id);

  return [
    factsSection('given', agent.name, givenRows(agent, run)),
    factsSection('produced', agent.name, producedRows(agent, run)),
    {
      id: 'transcript',
      heading: 'What ' + agent.name + ' said',
      rows: [],
      lines,
      empty: lines.length === 0 ? NOTHING.transcript : null,
    },
  ];
}
