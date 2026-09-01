import type { ReactElement } from 'react';

export const REFLECTION_LABEL = 'Learn from this run';

/**
 * Co się stanie, jeśli człowiek zostawi tę kontrolkę włączoną.
 *
 * 2026-08-29 (T-165) — DO DZIŚ STAŁ TU SAM PTASZEK. Kontrolka nazywała czynność („learn"),
 * a nie jej skutek, więc jedyną drogą do odpowiedzi na pytanie „co on z tym zrobi" było
 * przeczytanie `commands::run::what_this_run_taught_us`. Zdanie mówi trzy rzeczy, których
 * z samego napisu nie da się zgadnąć: że powstają NOTATKI, że jest ich najwyżej trzy
 * (`AT_MOST_KEPT` po tamtej stronie) i że nie wchodzą do niczego, dopóki człowiek ich nie
 * przyjmie w sekcji Knowledge.
 *
 * 2026-09-01 — JEDNO ZDANIE, ALE JUŻ NIE Z POWODU PASKA. Do dziś stało tu „pasek loadoutu ma
 * 52 px i tyle mu zostaje" — i było to uzasadnienie długości zdania miejscem, w którym zdanie
 * miało ZERO pikseli szerokości (pomiar przy [`ReflectionToggle`]). Zdanie zostaje jedno, bo
 * odpowiada na jedno pytanie; miejsce, w którym stoi, mieści dziś całe.
 */
export const REFLECTION_EXPLAINED =
  'Left on, it keeps up to three notes from this run for you to approve in Knowledge.';

/**
 * Czym to zdanie jest dla ptaszka: OPISEM, nigdy jego nazwą.
 *
 * 2026-08-29 — ROZRÓŻNIENIE ZMIERZONE, NIE TEORETYCZNE. Pierwsza wersja postawiła zdanie
 * WEWNĄTRZ `<label>`, obok napisu — i nazwa dostępna ptaszka stała się przez to całym
 * akapitem, bo nazwą kontrolki zawiniętej w etykietę jest cała treść tej etykiety. Zobaczyło
 * to cudze kryterium `e2e/tests/t126-reflection-choice-real-routes.spec.ts`, które szuka
 * kontrolki po nazwie DOKŁADNEJ, i wywaliło się siedem razy. Czytający ekranem usłyszałby to
 * samo, co ten selektor: pole wyboru o nazwie długości zdania.
 *
 * Zdanie stoi więc OBOK etykiety i wraca do kontrolki przez `aria-describedby` — czyli tam,
 * gdzie mieszkają opisy. Napis nazywa, opis wyjaśnia; jedno pole na jedno i jedno na drugie.
 */
const EXPLAINED_ID = 'reflection-explained';

export interface ReflectionToggleProps {
  readonly enabled: boolean;
  readonly disabled?: boolean;
  readonly onChange: (enabled: boolean) => void;
}

/**
 * Wybór, czy Loadout ma się z tego biegu czegoś nauczyć — RAZEM ze zdaniem, co to znaczy.
 *
 * ── 2026-09-01: DLACZEGO TA KONTROLKA ZESZŁA Z PASKA ────────────────────────────────────────
 *
 * ZMIERZONE w chromium na `e2e/harness.ts`, okno 1512×950, nawigacja rozwinięta (308 px),
 * scena z jednym workspace, jednym agentem i jednym workflow. Rząd kontrolek paska dostaje
 * **1108 px**. Chce **1562**. Cały niedobór — 454 px — to DWA NAPISY tej jednej kontrolki:
 *
 *     zdanie „Left on, it keeps…"     0 px z 400   (`truncate` zjadł je w całości)
 *     nazwa „Learn from this run"    57 px z 112   (czytała się „Learn f…")
 *
 * Reszta rzędu mieści się co do piksela: Copy diagnostics 126, wybór lidera 176, pole zadania
 * 128, `Run workflow` 114, „ile naraz" 261, sufit wydatku 168, odstępy 24. Bez tej kontrolki
 * rząd chce 1013 px i ma 95 px zapasu — czyli mieści się BEZ skracania czegokolwiek.
 *
 * DLACZEGO WŁAŚNIE TA, a nie `Copy diagnostics` (jedyna druga rzecz w tym rzędzie, która nie
 * jest polityką biegu). Zdjęcie diagnostyki oddaje 138 px przy niedoborze 454 — po niej zdanie
 * dalej miałoby zero pikseli. To nie jest wybór między dwiema kontrolkami, tylko wniosek
 * z pomiaru: pasek ma 52 px wysokości i JEDEN rząd, a ta jedna kontrolka jest jedyną, której
 * nie da się zrozumieć z samej nazwy — potrzebuje zdania, a zdania w jednym rzędzie 52 px nie
 * ma gdzie postawić przy żadnym oknie, dla którego ten produkt jest rysowany.
 *
 * ZDANIE O ZEROWEJ SZEROKOŚCI JEST GORSZE NIŻ JEGO BRAK, i to jest powód, dla którego to nie
 * skończyło się skasowaniem zdania. Ekran, który „tłumaczy" napisem szerokim na zero pikseli,
 * wygląda z markupu na produkt, który mówi człowiekowi, co robi — a człowiek nie widzi ani
 * litery. Kryterium `./reflection-explains-itself.test.tsx` było nad tym ZIELONE przez trzy dni:
 * pytało o tekst w markupie, bo repo nie ma jsdom i nie ma czym zapytać o szerokość. Odpowiedź
 * poszła więc przez chromium (`e2e/tests/what-this-run-keeps-is-readable.spec.ts`), a nie przez
 * rozluźnienie tamtego pytania.
 *
 * ── GDZIE STOI DZIŚ I DLACZEGO TAM ──────────────────────────────────────────────────────────
 *
 * U STOPY KOLUMNY PLANU (`../index.tsx`, `[data-plan-column]`), pod ścieżką kroków. Trzy powody,
 * w kolejności ważności:
 *
 *   1. TO JEST FAKT O KOŃCU BIEGU, nie o jego starcie. Prywatna tura idzie, kiedy ostatni krok
 *      zzielenieje — czyli odpowiada na to samo pytanie, co karta `../graph/after-run.tsx`
 *      („co się stanie, kiedy oni skończą"), i stoi w tej samej kolumnie, co ona.
 *   2. KOLUMNA MA 376 px I JEST KOLUMNĄ, więc zdanie się ZAWIJA. Każde miejsce, w którym
 *      zdanie dzieli JEDEN wiersz z czymkolwiek innym, jest o jedną długą nazwę workspace od
 *      tej samej wady — a wada polega właśnie na tym, że zdanie ustąpiło i nikt tego nie
 *      zgłosił. Zawijanie jest jedynym układem, w którym nie ma czego uciąć.
 *   3. STOPA KOLUMNY, A NIE OGON ŚCIEŻKI. Karta „when the last step turns green" jedzie
 *      `tail`em i przewija się razem z krokami — słusznie, bo jest zdaniem o ostatnim kroku.
 *      To jest KONTROLKA, którą trzeba ustawić PRZED startem: na planie o trzydziestu dwóch
 *      krokach (`e2e/tests/t161-long-workflow-stays-inside-run.spec.ts`) stałaby wtedy poniżej
 *      całej listy i dałoby się do niej dojść wyłącznie przewijaniem (niezmiennik 16).
 *
 * GRANICA TEGO WYBORU, NAZWANA I SPRAWDZANA. Pasek rysuje się zawsze; kolumna planu znika,
 * kiedy setup nie jest skończony i cały obszar pracy należy do przewodnika pierwszego
 * uruchomienia (`../first-run.tsx`). Ta kontrolka znika razem z nią — i wolno jej, bo taki
 * ekran nie ma czym zacząć biegu: `welcomeIsTheWholeScreen` wymaga, żeby BRAKOWAŁO folderu,
 * agenta albo workflow, a bez workflow kontrolka startu jest wyłączona u źródła
 * (`../start.tsx`). Nie jest to obietnica w komentarzu: pyta o to trzeci punkt
 * `./reflection-explains-itself.test.tsx` i przewraca się, kiedy start na takim ekranie ożyje.
 *
 * CZEGO TU NIE MA: `title` ze zdaniem. Podpowiedź była drugą kopią tego samego napisu i miała
 * sens dokładnie tak długo, jak długo napis był ucinany. Nie jest ucinany — więc druga kopia
 * jest tylko drugim miejscem, w którym to samo zdanie może się rozjechać (niezmiennik 13).
 */
export function ReflectionToggle({
  enabled,
  disabled = false,
  onChange,
}: ReflectionToggleProps): ReactElement {
  return (
    <div
      data-learn-choice
      /* `shrink-0`: stopa kolumny nie oddaje wysokości ścieżce kroków — ścieżka ma własny
         wycinek i własne przewijanie (`../graph/graph.tsx`), a kontrolka wyciśnięta do zera
         wysokości jest tą samą wadą, którą to przeniesienie zamyka, tylko w drugiej osi.
         `border-t`: kreska oddziela ją od ścieżki, tak samo jak `border-r` oddziela całą
         kolumnę od strumienia. */
      className="shrink-0 border-t border-line px-[14px] py-3"
    >
      {/* PTASZEK ZOSTAJE W ETYKIECIE, nie przy `htmlFor`: cudze kryterium sięga po niego
          selektorem `label:has-text(…) input[type="checkbox"]`
          (`e2e/tests/t161-long-workflow-stays-inside-run.spec.ts`), a poza tym kliknięcie
          w napis ma przełączać ptaszek i tak robi to samo zawinięcie.

          ANI JEDNEGO `truncate` I ANI JEDNEGO `shrink` w tej kontrolce — to nie jest
          przeoczenie, tylko cała różnica względem wersji z paska. Tam obie te klasy stały na
          obu napisach i to one zamieniły zdanie w zero pikseli. */}
      <label className="flex items-start gap-2 text-ui text-ink">
        {/* `mt-[3px]`: kwadrat ptaszka ma 13 px, wiersz napisu więcej, więc bez tego pole
            wisiałoby nad pierwszą literą zamiast stać w jej linii. */}
        <input
          type="checkbox"
          className="mt-[3px] shrink-0"
          checked={enabled}
          disabled={disabled}
          aria-describedby={EXPLAINED_ID}
          onChange={(event) => {
            onChange(event.target.checked);
          }}
        />
        <span>{REFLECTION_LABEL}</span>
      </label>

      {/* ZDANIE POD NAZWĄ, WCIĘTE POD JEJ PIERWSZĄ LITERĘ (13 px ptaszka + 8 px odstępu), żeby
          było widać, do czego należy. Stopień `label`, bo to jest opis kontrolki, a nie druga
          nazwa — czytelnik ma najpierw przeczytać, co włącza, a potem co to znaczy. */}
      <p id={EXPLAINED_ID} className="label mt-1 pl-[21px]">
        {REFLECTION_EXPLAINED}
      </p>
    </div>
  );
}
