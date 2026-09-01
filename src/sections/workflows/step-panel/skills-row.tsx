/* Wiersz Skills — obiecuje dokładnie tyle, ile potrafi CLI.
 *
 * Tryb przychodzi PROPSEM, choć w aplikacji jest jedną stałą (`SKILL_SUBSETTING`
 * w `capabilities.ts`). To nie jest nadmiarowość: dzięki temu wynik spike'u S-1 zmienia jedną
 * linię i zero testów, a oba warianty da się sprawdzić w jednym biegu.
 *
 * `'all-or-none'` znaczy, że „Only these" NIE ISTNIEJE — nie jest wyszarzone. Kontrolka
 * wyszarzona dalej obiecuje funkcję, tylko „na później"; kontrolka, która niczego nie zapisuje,
 * to niezmiennik 16 i anty-wzorzec „UI zbudowane na polu, którego nie ma" (FOUNDATIONS §6).
 *
 * Przy agencie na Codeksie całego wiersza nie ma: Codex nie ma pojęcia umiejętności
 * [T3 §7.2, T4 fakt-check O4]. Wiersz włączony, który nic nie robi, jest gorszy niż jego brak,
 * bo wygląda tak samo jak działający.
 */
import type { ReactElement } from 'react';
import type { Vendor } from '../../../state/agents';
import type { SkillChoice, Skills } from '../../../state/workflows';
import type { SkillMode } from './capabilities';

export interface SkillsRowProps {
  mode: SkillMode;
  /** Vendor AGENTA, którego wybrano na tym kroku. Codex nie ma umiejętności. */
  runsWith: Vendor;
  /** Umiejętności, które da się wskazać. Puste w trybie `all-or-none`. */
  available: string[];
  /** Wartość efektywna kroku. */
  value: Skills;
  onChoose: (choice: SkillChoice) => void;
}

/* `ROW`, `LABEL` i `NOTE` zniknęły 2026-08-31: rolę niosą `.stack`, `.label` i `.lead`
 * z `theme.css`. `CHOICE` zostaje — to jest KLEJ UKŁADU (pole wyboru obok zdania), a klej
 * prymityw celowo nie wchłania (DESIGN §6). */
const CHOICE = 'flex items-baseline gap-2 text-body text-ink';

/**
 * Czy ten wiersz ma po co powstać. Jedna odpowiedź, dwóch czytelników: sam wiersz i panel,
 * który liczy, ile rzeczy stoi za ujawnieniem.
 *
 * Codex nie ma pojęcia umiejętności [T3 §7.2, T4 fakt-check O4]. Pusty katalog znaczy to samo
 * z drugiej strony: przy zerze zainstalowanych umiejętności „all" i „none" robią dokładnie to
 * samo, a przełącznik między dwoma identycznymi skutkami jest kontrolką bez skutku
 * (niezmiennik 16). Do 2026-08-31 ta druga połowa stała w panelu, a pierwsza tutaj — czyli
 * jedna decyzja w dwóch plikach.
 */
export function skillsRowStands(runsWith: Vendor, available: readonly string[]): boolean {
  return runsWith !== 'codex' && available.length > 0;
}

/** Zdanie zwiniętej listy: ile jest do wzięcia i ile już wzięto.
 *
 * 2026-08-31 — POWSTAŁO, BO LISTA NIE MA SUFITU. Umiejętności przychodzą z cudzych katalogów,
 * więc ich liczba nie jest niczym ograniczona: repozytorium z trzydziestoma rozwijało tu
 * trzydzieści pól wyboru w kolumnie 330 px, zawsze i bez ostrzeżenia. „Ile" jest odpowiedzią,
 * której człowiek potrzebuje najczęściej, i teraz stoi przed listą, a nie za nią. */
function saysWhenShut(available: readonly string[], picked: ReadonlySet<string>): string {
  const offered = `${String(available.length)} to choose from`;
  return picked.size === 0 ? offered : `${offered}, ${String(picked.size)} picked`;
}

export function SkillsRow({
  mode,
  runsWith,
  available,
  value,
  onChoose,
}: SkillsRowProps): ReactElement | null {
  if (!skillsRowStands(runsWith, available)) return null;

  const picked = new Set(Array.isArray(value) ? value : []);

  /* Zaznaczenie pola wyboru przepisuje CAŁĄ listę w kolejności `available`, a nie dokłada
   * do końca. Kolejność klikania nie jest decyzją użytkownika, a zapisana do pliku wyglądałaby
   * jak zmiana przy każdym otwarciu tego wiersza [T3 §8.2]. */
  const toggle = (skill: string) => {
    onChoose({
      only: available.filter((one) => (one === skill ? !picked.has(one) : picked.has(one))),
    });
  };

  return (
    <div data-row="skills" className="stack">
      <span className="label">Skills</span>

      <label className={CHOICE}>
        <input
          type="radio"
          name="step-skills"
          checked={value === 'all'}
          onChange={() => {
            onChoose('all');
          }}
        />
        All skills
      </label>

      {mode === 'subset' ? (
        <>
          <label className={CHOICE}>
            <input
              type="radio"
              name="step-skills"
              checked={Array.isArray(value)}
              onChange={() => {
                onChoose({ only: [...picked] });
              }}
            />
            Only these
          </label>

          {/* LISTA ZA UJAWNIENIEM, licznik przed nim — patrz `saysWhenShut`. Rozwijanie jest
              zachowaniem przeglądarki, więc pola wyboru stoją w drzewie od pierwszego renderu
              i nie potrzebują ani handlera, ani stanu (niezmiennik 16). */}
          <details className="ml-4 rounded-md border border-line p-2">
            <summary className="label cursor-pointer">{saysWhenShut(available, picked)}</summary>
            <div className="stack pt-2">
              {available.map((skill) => (
                <label key={skill} className={CHOICE}>
                  <input
                    type="checkbox"
                    checked={picked.has(skill)}
                    onChange={() => {
                      toggle(skill);
                    }}
                  />
                  {skill}
                </label>
              ))}

              {/* Zmierzone w S-1: szesnastu umiejętności wbudowanych w Claude Code nie da się
                  zdjąć niczym poza flagą, która kasuje wszystkie do zera. Lista wyżej rządzi
                  dokładnie tymi, które da się zabrać — i tyle wolno obiecać. */}
              <span className="lead">Claude Code always keeps the ones it brings with it.</span>
            </div>
          </details>
        </>
      ) : (
        <label className={CHOICE}>
          <input
            type="radio"
            name="step-skills"
            checked={Array.isArray(value)}
            onChange={() => {
              onChoose('none');
            }}
          />
          No skills
        </label>
      )}
    </div>
  );
}
