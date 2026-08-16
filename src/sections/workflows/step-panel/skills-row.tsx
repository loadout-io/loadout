/* Wiersz Skills — obiecuje dokładnie tyle, ile potrafi CLI.
 *
 * SZKIELET — ciało rzuca `not implemented` (AGENTS.md §2a, odpowiednik `todo!()`).
 *
 * Tryb przychodzi PROPSEM, choć w aplikacji jest jedną stałą (`SKILL_SUBSETTING`
 * w `capabilities.ts`). To nie jest nadmiarowość: dzięki temu wynik spike'u S-1 zmienia jedną
 * linię i zero testów, a oba warianty da się sprawdzić w jednym biegu.
 *
 * `'all-or-none'` znaczy, że „Only these" NIE ISTNIEJE — nie jest wyszarzone. Kontrolka
 * wyszarzona dalej obiecuje funkcję, tylko „na później"; kontrolka, która niczego nie zapisuje,
 * to niezmiennik 16 i anty-wzorzec „UI zbudowane na polu, którego nie ma" (00-SYNTHESIS §6).
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

const ROW = 'flex flex-col gap-1';
const LABEL = 'text-label text-muted';
const CHOICE = 'flex items-baseline gap-2 text-body text-ink';
const NOTE = 'text-label text-muted';

export function SkillsRow({
  mode,
  runsWith,
  available,
  value,
  onChoose,
}: SkillsRowProps): ReactElement | null {
  /* Codex nie ma pojęcia umiejętności [T3 §7.2, T4 fakt-check O4], więc wiersza NIE MA.
   * Wyszarzony dalej obiecuje, że kiedyś zadziała, a nikt nie przyjdzie go włączyć. */
  if (runsWith === 'codex') return null;

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
    <div className={ROW}>
      <span className={LABEL}>Skills</span>

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

          {available.map((skill) => (
            <label key={skill} className={`${CHOICE} pl-4`}>
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

          {/* Zmierzone w S-1: szesnastu umiejętności wbudowanych w Claude Code nie da się zdjąć
              niczym poza flagą, która kasuje wszystkie do zera. Lista wyżej rządzi dokładnie
              tymi, które da się zabrać — i tyle wolno obiecać. */}
          <span className={NOTE}>Claude Code always keeps the ones it brings with it.</span>
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
