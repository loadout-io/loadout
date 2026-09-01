import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-3 dla T-46: plywa dokladnie JEDNA rzecz, i przy „mniej przejrzystosci" szklo znika w calosci.
 *
 * DESIGN §3 mowi: cien wylacznie pod tym, co PLYWA. W calej aplikacji plywa jedna rzecz — panel
 * nawigacji. Glebia wewnatrz strony bierze sie ze zmiany powierzchni, nie z cienia. Reguła nie
 * jest estetyczna: „glebia wszedzie" jest dokladnie tym, co zamienia szklo w ozdobe, a wtedy
 * przestaje cokolwiek znaczyc, ze jedna rzecz jest nad pozostalymi.
 *
 * DLACZEGO LISTA REGUL JEST CZYTANA, NIE WPISANA. Sprawdzenie, ktore pyta „czy `.nav` ma cien",
 * nie mowi nic o tym, ile innych rzeczy tez go ma. Ten test wylicza WSZYSTKIE reguly makiety
 * i odejmuje wylacznie te, ktore makieta sama nazywa swoim rusztowaniem (pasek `.mockbar`
 * i noty projektowe `.an`) — reszta jest aplikacja i podlega regule.
 *
 * Cudzyslowy w wyrazeniach regularnych zapisane heksadecymalnie (\x22, \x27): literal
 * o nieparzystej ich liczbie rozsynchronizowuje skaner `checks/quick-vocabulary.sh`.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

function withoutComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, ' ');
}

interface Rule {
  readonly selector: string;
  readonly body: string;
}

/** Ile regul arkusz w ogole ma: kazde `{`, ktore nie otwiera reguly `@`. */
function ruleCount(css: string): number {
  return [...css.matchAll(/([^{}]*)\{/g)].filter((hit) => !/@[a-z-]+/.test(hit[1] ?? '')).length;
}

/** Wszystkie reguly arkusza makiety poza jej wlasnym rusztowaniem.
 *
 * PODZIAL NA `}`, a nie wzorzec globalny — poprawione po drugiej opinii 2026-08-19.
 * Poprzednia wersja szla wzorcem `/(^|\})\s*([^{}@]+?)\s*\{([^}]*)\}/g`, ktory ZJADA
 * domykajacy nawias kazdej dopasowanej reguly i JEDNOCZESNIE wymaga nawiasu przed nastepnym
 * selektorem. Skutek: reguly wpadaly naprzemiennie, czyli ten punkt przeszukiwal POLOWE
 * arkusza — a punkt o liczbie regul tego nie widzial, bo polowa ze stu pieciudziesieciu
 * to wciaz wiecej niz dwadziescia. Cien podnoszacy dopisany do reguly na pominietej
 * parzystosci przechodzil zielono, czyli dokladnie ta „glebia wszedzie", przed ktora
 * to kryterium stoi. Gorzej: wstawienie albo usuniecie JAKIEJKOLWIEK reguly wyzej
 * przestawialo parzystosc wszystkiego ponizej.
 */
function appRules(css: string): readonly Rule[] {
  const out: Rule[] = [];
  for (const chunk of css.split('}')) {
    const at = chunk.indexOf('{');
    if (at < 0) continue;
    const selector = chunk.slice(0, at).replace(/\s+/g, ' ').trim();
    const body = chunk.slice(at + 1);
    if (selector === '' || selector.startsWith('@')) continue;
    /* Rusztowanie makiety, ktore makieta sama tak nazywa: „pasek makiety: NIE jest czescia
     * aplikacji" oraz noty projektowe. Nie podlegaja regule o cieniach, bo nie sa aplikacja. */
    if (/mockbar/.test(selector)) continue;
    if (/(^|[\s,])\.an(\b|[.:[])/.test(selector)) continue;
    out.push({ selector, body });
  }
  return out;
}

/** Arkusz makiety, bez komentarzy. */
function sheet(html: string): string {
  return withoutComments(/<style>([\s\S]*?)<\/style>/.exec(html)?.[1] ?? '');
}

/** Czlony deklaracji `box-shadow`, po jednym na cien.
 *
 * PODZIAL LICZY NAWIASY, poprawione 2026-08-31. Stalo tu `split(/,(?![^(]*\))/)`, czyli „przecinek,
 * po ktorym nie ma zamkniecia nawiasu" — i to nie widzi ZAGNIEZDZENIA. W `color-mix(in srgb,
 * var(--c) 22%,transparent)` po pierwszym przecinku stoi `var(`, wiec wyprzedzenie nie trafia
 * i jedna barwa rozpadala sie na trzy czlony. Zaden punkt tego nie pokazywal, bo lista byla
 * skladana z powrotem przez `join`, a poskladany napis wyglada jak caly cien.
 */
function shadowParts(body: string): readonly string[] {
  const declared = /(?:^|;)\s*box-shadow\s*:([^;]*)/.exec(body)?.[1] ?? '';
  if (declared.trim() === '') return [];
  const out: string[] = [];
  let depth = 0;
  let current = '';
  for (const letter of declared) {
    if (letter === '(') depth += 1;
    if (letter === ')') depth -= 1;
    if (letter === ',' && depth === 0) {
      out.push(current);
      current = '';
      continue;
    }
    current += letter;
  }
  out.push(current);
  return out.map((part) => part.trim()).filter((part) => part !== '' && part !== 'none');
}

/** Slowa czlonu, z funkcja koloru trzymana w calosci.
 *
 * Podzial liczy nawiasy, a nie bialy znak: `color-mix(in srgb , var(--c) 22%,transparent)` ma
 * w srodku i spacje, i przecinki, wiec goly `split` rozsypalby jedna barwe na cztery „dlugosci"
 * i kazdy cien z taka barwa czytalby sie jako przesuniety.
 */
function words(part: string): readonly string[] {
  const out: string[] = [];
  let depth = 0;
  let current = '';
  for (const letter of part) {
    if (letter === '(') depth += 1;
    if (letter === ')') depth -= 1;
    if (depth === 0 && /\s/.test(letter)) {
      if (current !== '') out.push(current);
      current = '';
      continue;
    }
    current += letter;
  }
  if (current !== '') out.push(current);
  return out;
}

const LENGTH = /^-?[\d.]+[a-z%]*$/i;
const ZERO = /^-?0(?:[a-z%]+)?$/i;

/** Przesuniecia czlonu: pierwsze dwie dlugosci, bo tak stoi w gramatyce `box-shadow`. */
function offsets(part: string): readonly string[] {
  return words(part)
    .filter((word) => word !== 'inset' && LENGTH.test(word))
    .slice(0, 2);
}

/** Barwa czlonu: pierwsze slowo, ktore nie jest ani `inset`, ani dlugoscia. */
function colourOf(part: string): string {
  return words(part).find((word) => word !== 'inset' && !LENGTH.test(word)) ?? '';
}

/** Czy czlon jest BLASKIEM: oba przesuniecia zerowe, czyli swiatlo bez kierunku. */
function isGlow(part: string): boolean {
  const shift = offsets(part);
  return shift.length === 2 && shift.every((one) => ZERO.test(one));
}

/** Barwy, ktore nie niosa ani stanu, ani tozsamosci: czern, biel, kazda szarosc, brak barwy. */
function neutral(colour: string): boolean {
  if (colour === '') return true;
  if (/^(black|white|gr[ae]y|currentColor|transparent)$/i.test(colour)) return true;
  const hex = /^#([0-9a-f]{3,8})$/i.exec(colour)?.[1];
  if (hex !== undefined && (hex.length === 3 || hex.length === 4)) {
    return hex[0] === hex[1] && hex[1] === hex[2];
  }
  if (hex !== undefined && (hex.length === 6 || hex.length === 8)) {
    return hex.slice(0, 2) === hex.slice(2, 4) && hex.slice(2, 4) === hex.slice(4, 6);
  }
  const channels = /^rgba?\(([^)]*)\)$/i.exec(colour)?.[1];
  if (channels !== undefined) {
    const three = channels
      .split(/[,\s/]+/)
      .filter((one) => one !== '')
      .slice(0, 3);
    return three.length === 3 && three[0] === three[1] && three[1] === three[2];
  }
  return false;
}

/** PODNIESIENIA reguly: czlony, ktore naprawde klada rzecz NAD strona.
 *
 * DWIE ODJETE KLASY, obie z DESIGN §3 („Blask nie jest glebia", 2026-08-31):
 *
 *   `inset` — refleks na krawedzi szkla nigdy nie byl glebia i nigdy tu nie wpadal;
 *   BLASK (`0 0 <promien> <barwa>`) — swiatlo bez kierunku. Nie udaje zrodla z gory, wiec nie
 *     buduje warstw: swieci ta sama barwa, co stan albo tozsamosc, ktora niesie, i gasnie razem
 *     z nia. Do 2026-08-31 ta funkcja czytala KAZDY czlon bez `inset`, czyli sadzila regule,
 *     ktorej DESIGN juz nie stawia — i byla przez to czerwona na makiecie zgodnej z dokumentem.
 *
 * Blask nie jest zwolnieniem: punkt „a glow carries a colour" nizej pilnuje, zeby zerowe
 * przesuniecie nie stalo sie furtka dla czarnej poswiaty, czyli dla podniesienia napisanego
 * inaczej.
 */
function liftingShadows(body: string): readonly string[] {
  return shadowParts(body).filter((part) => !/^inset\b/.test(part) && !isGlow(part));
}

/** BLASKI reguly: czlony bez `inset`, ktore nie maja kierunku. */
function glows(body: string): readonly string[] {
  return shadowParts(body).filter((part) => !/^inset\b/.test(part) && isGlow(part));
}

describe('plywa dokladnie jedna rzecz', () => {
  const css = sheet(fileText(MOCKUP));
  const rules = appRules(css);

  it('read a real sheet out of the mockup', () => {
    expect(
      rules.length,
      'almost no rule was read out of the mockup style sheet, so every point below would loop ' +
        'over an empty list and pass on nothing',
    ).toBeGreaterThan(20);
  });

  it('reads EVERY rule, not every other one', () => {
    /* KONTROLA PRZECIW PARZYSTOSCI, dopisana po drugiej opinii. Sama liczba regul nie odroznia
     * „przeczytalem arkusz" od „przeczytalem co druga regule": polowa duzego arkusza jest wciaz
     * duza. Porownujemy wiec liczbe przeczytanych regul z liczba WSZYSTKICH otwarc reguly
     * w arkuszu, po odjeciu rusztowania makiety. */
    const total = ruleCount(css);
    expect(total, 'no rule opening was found in the sheet at all').toBeGreaterThan(20);
    const harness = total - rules.length;
    expect(
      harness,
      'the enumerator skipped ' +
        String(harness) +
        ' of ' +
        String(total) +
        ' rules while the mockup declares only a handful of harness ones. Half a sheet is still ' +
        'a big number, so counting rules cannot tell reading the sheet apart from reading every ' +
        'other rule of it — and a lifting shadow on a skipped one would pass green.',
    ).toBeLessThan(12);
  });

  it('lifts the navigation panel, so it really does float', () => {
    const nav = rules.find((rule) => rule.selector === '.nav');
    expect(nav, 'the mockup has no .nav rule at all').toBeDefined();
    expect(
      liftingShadows(nav?.body ?? ''),
      'the navigation panel carries no lifting shadow, so it is drawn as a pane that floats and ' +
        'reads as a pane that lies flat',
    ).not.toEqual([]);
  });

  it('lifts NOTHING else, because depth everywhere is depth nowhere', () => {
    const lifted = rules
      .filter((rule) => rule.selector !== '.nav')
      .filter((rule) => liftingShadows(rule.body).length > 0)
      .map((rule) => rule.selector + ' -> ' + liftingShadows(rule.body).join(' , '));
    expect(
      lifted,
      'these rules carry a lifting shadow and they do not float. DESIGN §3 keeps shadows for ' +
        'the one thing that is above the page; anything else gets its depth from a change of ' +
        'surface. Inset shadows are allowed everywhere — a gleam on the edge of glass is not depth.',
    ).toEqual([]);
  });

  it('lets a glow carry a colour, never the black one that lifting is written in', () => {
    /* DRUGA POLOWA REGULY z DESIGN §3, dopisana 2026-08-31 razem z rozroznieniem blask/podniesienie.
     * Bez niej „zerowe przesuniecie" byloby furtka: `0 0 24px rgba(0,0,0,.5)` klamie o kierunku
     * i robi dokladnie to, co cien podnoszacy — kladzie rzecz nad strona. Dokument mowi to wprost:
     * blask ma barwe tokenu stanu albo tozsamosci i gasnie razem z nim, a blask w kolorze
     * neutralnym jest podniesieniem napisanym inaczej.
     *
     * Ten punkt jest tez KONTROLA nad punktem wyzej: to on trzyma sume obu regul na tym samym
     * poziomie, na ktorym stala jedna regula przed rozdzieleniem. */
    const all = rules.flatMap((rule) =>
      glows(rule.body).map((part) => ({ rule, part, colour: colourOf(part) })),
    );
    expect(
      all.length,
      'not one glow was read out of the mockup, so this point would demand nothing of anything',
    ).toBeGreaterThan(0);
    expect(
      all.filter((one) => neutral(one.colour)).map((one) => one.rule.selector + ' -> ' + one.part),
      'these glows carry no colour of their own. A glow says "this is live", "this is done", ' +
        '"this is that agent" — it borrows the colour of the thing it belongs to and goes out ' +
        'with it. A neutral one says nothing and only lifts, which is the rule above written ' +
        'the other way round.',
    ).toEqual([]);
  });

  it('turns ALL the glass solid when the reader asked for less transparency', () => {
    /* POPRAWIONE po drugiej opinii 2026-08-19. Poprzednia wersja sprawdzala, czy blok ZAWIERA
     * nazwy trzech selektorow — czyli przechodzila na bloku, ktory ustawia im `border-radius: 0`
     * i nie zdejmuje ani rozmycia, ani przejrzystosci. Lista szklanych powierzchni byla przy tym
     * WPISANA w test, wiec czwarta byla zwolniona z reguly przez samo powstanie.
     *
     * Teraz: lista jest CZYTANA z arkusza (szklana jest kazda regula deklarujaca rozmycie poza
     * blokiem), a od bloku wymagamy dwoch rzeczy naraz — krycącego tla i zdjetego rozmycia. */
    const blockAt = css.indexOf('@media (prefers-reduced-transparency');
    expect(
      blockAt,
      'the mockup has no prefers-reduced-transparency block. It is a HIG requirement and the ' +
        'design system next door enforces it: a reader who turned transparency off gets solid ' +
        'panes no matter what the design wants.',
    ).toBeGreaterThan(-1);
    const block = css.slice(blockAt, css.indexOf('\n}', blockAt) + 2);

    /* Szklana jest kazda regula, ktora deklaruje rozmycie — poza samym blokiem, ktory je zdejmuje. */
    const glass = appRules(css.slice(0, blockAt) + css.slice(blockAt + block.length))
      .filter((rule) => /backdrop-filter\s*:\s*(?!none)/.test(rule.body))
      .flatMap((rule) => rule.selector.split(',').map((one) => one.trim()));
    expect(
      glass.length,
      'no glass surface was found in the sheet, so this point would demand nothing of the block',
    ).toBeGreaterThan(0);

    const missed = glass.filter((selector) => !block.includes(selector));
    expect(
      missed,
      'these glass surfaces are not named by the reduced-transparency block. Turning one of ' +
        'three solid is worse than turning none: the window then mixes two materials for one ' +
        'kind of surface.',
    ).toEqual([]);

    expect(
      /background\s*:\s*var\(--solid\)/.test(block),
      'the block names the surfaces and never makes them opaque. A block that only changes a ' +
        'corner radius satisfies "the selectors are listed" and leaves the reader with blurred ' +
        'chrome, which is exactly the HIG requirement this point cites.',
    ).toBe(true);
    expect(
      /backdrop-filter\s*:\s*none/.test(block),
      'the block leaves the blur on. Blurring an opaque background costs GPU and changes not a ' +
        'single pixel, so dropping transparency without dropping the blur is pure waste.',
    ).toBe(true);
  });
});
