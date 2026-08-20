/* Strzałka w górę cofa do poprzedniej linii — i nie gubi tego, co człowiek właśnie pisał.
 *
 * Zgłoszenie właściciela 2026-08-20: „strzałka w gore nie cofa do poprzedniej linii". Wiersz
 * wejścia nazywa się terminalem, a jedyną drogą do powtórzenia komendy jest przepisanie jej
 * z pamięci — przy `/run <workflow> <całe zdanie zadania>` to jest przepisywanie akapitu.
 *
 * CZYSTY MODUŁ, bo to repo nie ma jsdom i Enter jest dla kryterium nieosiągalny — to samo
 * rozumowanie, co przy `suggestions` i `../run-command.ts`.
 *
 * SŁABA WERSJA: test wyłącznie na chodzeniu wstecz. Przechodzi dla implementacji, która przy
 * kroku naprzód CZYŚCI pole — czyli kasuje zdanie, które człowiek właśnie pisał, i robi to
 * cicho, bez śladu na ekranie. Rozróżnia to przypadek ze szkicem, i dlatego stoi w tym samym
 * pliku, a nie „gdzieś obok".
 */
import { describe, expect, it } from 'vitest';

import { HISTORY_LIMIT, createHistory } from './history';

describe('the arrow walks back to the line before, and never eats the half-written one', () => {
  it('hands over the newest line first, the one before it second, and walks forward again', () => {
    const walk = createHistory();
    walk.remember('/run easy make the reader strict');
    walk.remember('/open');
    walk.remember('what are you doing right now');

    expect(
      walk.back(''),
      'the first step back has to hand over the line that was sent LAST. A history that starts ' +
        'at the oldest entry answers a question nobody asked: the line a person wants again is ' +
        'almost always the one they just wrote.',
    ).toBe('what are you doing right now');
    expect(
      walk.back('what are you doing right now'),
      'the second step back has to reach the line before that one. Without this, "the arrow ' +
        'works" means one line deep, which is a repeat key, not a history.',
    ).toBe('/open');
    expect(
      walk.forward(),
      'a step forward has to walk the other way, back towards the newest line. An arrow that ' +
        'only goes one direction leaves the person stuck at the oldest thing they ever typed.',
    ).toBe('what are you doing right now');
  });

  it('walking forward past the newest line gives back the half-written one, not an empty field', () => {
    const walk = createHistory();
    walk.remember('/stop');
    walk.remember('/open');

    /* To zdanie jest sednem tego pliku: człowiek zaczął pisać, sięgnął wstecz po komendę
     * i wraca. Puste pole w tym miejscu jest cichym skasowaniem jego zdania. */
    const halfWritten = 'tell me what is left to do before I';

    expect(walk.back(halfWritten), 'the first step back reaches the newest line').toBe('/open');
    /* DRUGI KROK WSTECZ PODAJE TO, CO STOI W POLU TERAZ — czyli cudzą linię, nie szkic.
     * Implementacja, która zapamiętuje szkic przy KAŻDYM kroku wstecz, gubi go dokładnie
     * tutaj, a to jest ta wersja wady, której samo chodzenie wstecz nie widzi. */
    expect(walk.back('/open'), 'the second step back reaches the line before it').toBe('/stop');
    expect(walk.forward(), 'and forward walks back up to the newest line').toBe('/open');

    expect(
      walk.forward(),
      'coming forward past the newest line has to give back the sentence that was in the field ' +
        'before the first step back. An implementation that clears the field here deletes what ' +
        'a person was writing, and leaves nothing on the screen to say it happened. This is the ' +
        'whole reason the draft travels into the walk instead of living in the field alone.',
    ).toBe(halfWritten);
  });

  it('two identical lines in a row take one place', () => {
    const walk = createHistory();
    walk.remember('/open');
    walk.remember('/stop');
    walk.remember('/stop');

    expect(walk.back(''), 'the newest line is still the newest line').toBe('/stop');
    expect(
      walk.back('/stop'),
      'repeating a command is ordinary here, so the same sentence twice in a row has to take one ' +
        'place. Otherwise walking back crosses the same words twice and stops answering the ' +
        'question "what did I do before this".',
    ).toBe('/open');
  });

  it('has a ceiling, and it is the OLDEST line that falls out of it', () => {
    expect(
      Number.isInteger(HISTORY_LIMIT) && HISTORY_LIMIT > 0 && HISTORY_LIMIT < 10_000,
      'the ceiling has to be a real, finite count of lines. Without one this file would either ' +
        'walk forever or measure a limit that no window ever reaches.',
    ).toBe(true);

    const walk = createHistory();
    const oldest = 'line 0';
    walk.remember(oldest);
    for (let n = 1; n <= HISTORY_LIMIT; n += 1) walk.remember('line ' + String(n));

    /* Chodzimy DOKŁADNIE tyle razy, ile wpisów ma się zmieścić: każdy krok odwiedza jeden
     * z zapamiętanych wpisów, więc test nie zależy od tego, co robi krok poza najstarszym. */
    const seen: string[] = [];
    let field = '';
    for (let step = 0; step < HISTORY_LIMIT; step += 1) {
      const back = walk.back(field);
      if (back === null) break;
      field = back;
      seen.push(back);
    }

    expect(
      seen.length,
      'walking back as many times as the ceiling allows has to reach that many lines. Fewer ' +
        'means the ceiling is smaller than it says, which makes the assertion below a statement ' +
        'about a shorter history than the one this module promises.',
    ).toBe(HISTORY_LIMIT);
    expect(
      seen,
      'the line that fell out has to be the OLDEST one. An implementation that drops the newest ' +
        'keeps the count right and throws away exactly the line a person is about to ask for.',
    ).not.toContain(oldest);
    expect(
      seen.at(-1),
      'and the deepest step has to reach the second-oldest line, so nothing between the two ends ' +
        'quietly went missing as well',
    ).toBe('line 1');
  });

  it('with nothing remembered, a step back hands over nothing at all', () => {
    const walk = createHistory();

    expect(
      walk.back('half a sentence nobody sent yet'),
      'with an empty history the step back has to hand over nothing, so the field is left ' +
        'exactly as it was. An empty string here is not "nothing": it is written into the ' +
        'field, and it wipes the sentence a person is in the middle of.',
    ).toBeNull();
  });
});
