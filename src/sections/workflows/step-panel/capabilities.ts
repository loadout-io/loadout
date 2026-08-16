/* Ile kontrolka Skills ma prawa obiecać — jedna stała, jedno miejsce.
 *
 * Wynik spike'u S-1 (`docs/research/topics/S1-skill-subsetting.md`, zmierzone 2026-08-15,
 * `claude 2.1.233`): podzbiór umiejętności na sesję JEST prawdziwy i wymaga katalogu
 * generowanego plus dwóch flag, nie jednej — `--plugin-dir <katalog>` (dokłada nasze) razem
 * z `--setting-sources ""` (usuwa cudze). Zmierzony ciąg: 54 → 18. Każda z flag osobno zawodzi:
 * sam `--plugin-dir` daje 54 → 56 (to dokładanie, nie filtr), sam `--setting-sources ""` daje
 * 54 → 16 (obcięcie do podłogi, której nie kontrolujemy).
 *
 * Zastrzeżenie, które zmienia copy, a nie tę stałą: 16 umiejętności wbudowanych w CLI przeżywa
 * `--setting-sources ""` i nie da się ich zdjąć niczym poza `--disable-slash-commands`, które
 * kasuje wszystko do zera. Uczciwa obietnica brzmi więc „tylko te, plus te, które CLI ma
 * własne" — lista pól wyboru rządzi dokładnie tymi umiejętnościami, które da się zabrać.
 *
 * Dlaczego mimo to STAŁA, a nie po prostu wpisany tryb: gdyby S-1 wypadł źle, ta linia zmienia
 * się na `'all-or-none'` i zero testów. Komponent bierze tryb PROPSEM, więc kryterium sprawdza
 * oba warianty niezależnie od tego, jak spike wypadł.
 */

/** `'subset'` — „All skills" / „Only these" z listą pól wyboru.
 *  `'all-or-none'` — „All skills" / „No skills" i nic pomiędzy. */
export type SkillMode = 'subset' | 'all-or-none';

export const SKILL_SUBSETTING: SkillMode = 'subset';
