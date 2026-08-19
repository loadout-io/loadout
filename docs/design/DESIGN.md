# Loadout — system projektowy

Wersja 1 · 2026-08-15 · wiążąca dla całego frontendu

Ten plik jest źródłem prawdy dla wyglądu. Kod nie zawiera literałów kolorów ani rozmiarów —
tylko odwołania do tokenów zdefiniowanych tutaj. Agent, który dodaje komponent, czyta ten plik pierwszy.

---

## 1. Teza

**Widok pracy nie przyrasta. Aktualizuje się w miejscu.**

Zwykły terminal dopisuje na dół w nieskończoność. Po dziesięciu minutach pracy czterech agentów
masz ścianę tekstu, w której nie widać, co się dzieje teraz. Poprzednia wersja (poprzedni prototyp) dokładnie
tak działała i to jest główna przyczyna, dla której jej UI był przytłaczający.

Loadout dzieli widok na dwie strefy o różnej fizyce:

```
┌─────────────────────────────────────────────┐
│  HISTORIA — przyrasta, jedna linia na krok  │  ← rośnie w dół, zwięzła
│  ✓ Plan          4 steps                    │
│  ✓ Research      3 agents · 2m              │
├─────────────────────────────────────────────┤
│  TERAZ — stała wysokość, nadpisywana        │  ← nie rośnie, mutuje
│  Forge    writing   src/parser.rs           │
│  Needle   tests     12 ✓  0 ✗               │
│  Rivet    waiting   on Needle               │
├─────────────────────────────────────────────┤
│  ❯ _                                        │
└─────────────────────────────────────────────┘
```

*(Teksty w UI są po angielsku — decyzja D5. Opisy i komentarze w dokumentacji po polsku.)*

Jeden agent = jedna linia, która się przepisuje. Jak `top`, nie jak `tail -f`.
Skończony krok zwija się do jednej linii historii. Nic nie znika — pełny zapis jest o kliknięcie dalej —
ale nic się też nie nawarstwia.

**Sprawdzian:** jeśli podczas biegu czterech agentów przez pięć minut widok przewinął się choć raz
sam z siebie — projekt jest złamany.

---

## 2. Podpis wizualny: pasek loadoutu

Nazwa aplikacji pochodzi z gier: loadout to zestaw, który kompletujesz **przed** wyjściem w teren.
To jest jedyny ozdobny element w całej aplikacji i jedyne miejsce, gdzie wolno nam być efektowni.

Nad widokiem pracy siedzi pasek — wybrany workflow jako ciąg bloków, jeden na krok:

```
▓▓▓▓  ▓▓▓▓  ░░░░  ░░░░      Exact-diff delivery · krok 2 z 4
 plan  bada  pisze sprawdza
```

- Blok skończony: wypełniony, `--ink-muted`
- Blok aktywny: wypełniony `--accent`, jedyny nasycony element na ekranie
- Blok czekający: obrys `--line`, puste wnętrze

Pasek jest jednocześnie nawigacją: klik w blok pokazuje historię tego kroku.
Nie ma paska postępu w procentach. Kroki to nie procenty.

---

## 3. Kolor

Wartości pochodzą z systemu projektowego meetnotes (`../meetnotes/src/design-tokens/`, nazwa
własna „Quiet Glass"). Bierzemy **wartości**, nie inspirację — dwie nasze aplikacje mają w Docku
wyglądać na rodzeństwo (decyzja D1). Kopia wartości leży w `docs/design/house-values.json`
i jest porównywana z `theme.css` w każdym biegu testu.

Reguła nadrzędna całego systemu, wprost z tamtego pliku: **szkło jest chrome, treść jest
papierem.** Szkło nie wchodzi nigdy pod tekst ani pod kod, które człowiek ma przeczytać.

### Powierzchnie

| Token | Hex | Użycie |
|---|---|---|
| `--bg` | `#07070b` | tło aplikacji, kartka treści |
| `--panel` | `rgba(255, 255, 255, 0.045)` | wypełnienie szkła: menu, pasek, szyna |
| `--raised` | `rgba(255, 255, 255, 0.045)` | karty i kafelki na szkle |
| `--well` | `rgba(255, 255, 255, 0.035)` | pola wejściowe, bloki kodu |
| `--overlay` | `#1b1b24` | **nieprzejrzyste** menu i podpowiedzi |
| `--solid` | `#111118` | gdy potrzebna jest prawdziwie kryjąca powierzchnia |
| `--hover` | `rgba(255, 255, 255, 0.06)` | podkład wiersza pod kursorem |
| `--scrim` | `rgba(0, 0, 0, 0.5)` | przygaszenie za modalem |

Powierzchnie podniesione są **bielą-alfa**, nie własnym szarym. Jedna definicja daje dwa
zachowania: nad aurorą załamuje światło, nad kartką treści czyta się jako delikatne podniesienie.
Menu i podpowiedź **nie mogą** być szkłem — leżą nad treścią, którą człowiek właśnie czyta.

### Tekst

| Token | Hex | Użycie |
|---|---|---|
| `--ink` | `#f6f6fa` | nagłówki, wartości, aktywna treść |
| `--body` | `#a6a6b6` | zdania, opisy, treść domyślna |
| `--muted` | `#8a8a9c` | etykiety, metadane, rzeczy skończone |

Dom ma **cztery** stopnie tekstu; bierzemy trzy. `--muted` to domowy `--text-tertiary`, nie
`--text-muted`: tamten stopień (`#6c6c7d`) mierzy 3,62:1 na powierzchni podniesionej, czyli
**pod progiem czytelności**, i dom trzyma go wyłącznie dla ≥13 px. U nas prawie każdy przygaszony
napis to metadana ≤12 px, więc jaśniejszy stopień (5,50:1) jest jedynym poprawnym. Czwartego nie
wprowadzamy, żeby nikt nie miał czym sięgnąć po ciemniejszy.

### Linie

| Token | Hex |
|---|---|
| `--line` | `rgba(255, 255, 255, 0.09)` |
| `--line-strong` | `rgba(255, 255, 255, 0.16)` |
| `--line-subtle` | `rgba(255, 255, 255, 0.055)` |

### Akcent — mówi „to jest interaktywne" i nic więcej

| Token | Hex | Użycie |
|---|---|---|
| `--accent` | `#6e76ff` | focus, przycisk podstawowy, aktywny glif, kursor w polu |
| `--accent-hover` | `#8a90ff` | ten sam element pod kursorem |
| `--accent-active` | `#5b63f0` | ten sam element wciśnięty |
| `--accent-soft` | `rgba(110, 118, 255, 0.16)` | tło elementu wybranego |
| `--accent-ring` | `rgba(110, 118, 255, 0.5)` | obrys kontrolki, pierścień focusu |

Do 2026-08-19 jeden token odpowiadał na dwa różne pytania: ten dokument mówił jednocześnie
„`--accent` jest **jedynym kolorem interaktywnym**" i „`--accent` znaczy **teraz**". To są dwa
fakty, więc mają dwa tokeny (niezmiennik 13).

**Akcent nigdy nie wypełnia chrome.** Bierze go focus, przycisk podstawowy i aktywny glif —
i na tym koniec. Aktywny wiersz menu jest neutralny; barwę dostaje wyłącznie jego glif.

### Stan — cztery i ani jeden więcej

| Token | Hex | Znaczy | Pytanie, na które odpowiada |
|---|---|---|---|
| `--live` | `#ff7a5c` | **teraz** | co się dzieje w tej chwili? |
| `--attend` | `#f5b14c` | **ty** | co czeka na moją decyzję? |
| `--fail` | `#ff6b6b` | **zepsute** | co poszło źle? |
| `--human` | `#9d7bff` | **człowiek** | co zrobiła osoba, nie maszyna? |

Wash i edge dla każdego — tło chipa i jego obrys:

| Token | Hex | Token | Hex |
|---|---|---|---|
| `--live-soft` | `rgba(255, 122, 92, 0.16)` | `--live-edge` | `rgba(255, 122, 92, 0.5)` |
| `--attend-soft` | `rgba(245, 177, 76, 0.14)` | `--attend-edge` | `rgba(245, 177, 76, 0.5)` |
| `--fail-soft` | `rgba(255, 107, 107, 0.14)` | `--fail-edge` | `rgba(255, 107, 107, 0.5)` |
| `--human-soft` | `rgba(157, 123, 255, 0.14)` | `--human-edge` | `rgba(157, 123, 255, 0.5)` |

`--live` w domu nazywa się tak samo i pilnuje nagrywania; u nas pilnuje pracującego agenta.
Ta sama robota: **żywe, nie alarmujące.**

#### Reguła formy, bez której `--live` i `--fail` są nieodróżnialne

Te dwie barwy różnią się odcieniem o **~13°**, a w naszym strumieniu stoją w sąsiednich
wierszach — czego dom nigdy nie musi pokazać. Rozstrzyga to forma, nie barwa:

- `--live` występuje **wyłącznie** jako: podkład aktywnego wiersza strefy „teraz", jego obrys,
  aktywny segment paska loadoutu, pulsująca kropka, kropka karty w tle.
- `--fail` występuje **wyłącznie** jako: glif `✕`, obrys chipa, lewa krawędź bloku błędu.

Rozłączność tych dwóch słowników form jest **sprawdzana statycznie**, nie oceniana okiem.

### Tożsamość ≠ stan

Agenci mają swoje kolory, żeby szyna dała się skanować wzrokiem. Ale kolor agenta **nigdy nie
może być pomylony z kolorem stanu** — inaczej pomarańczowy agent i „czeka na twoją decyzję"
znaczą to samo. Rozdział jest po nasyceniu.

| | Nasycenie | Tokeny |
|---|---|---|
| **Stan** | nasycone | `--live` `--attend` `--fail` `--human` |
| **Tożsamość** | przygaszone, zbliżona jasność | `--id-1 #6f8496` `--id-2 #7f7597` `--id-3 #94886b` `--id-4 #6b9285` `--id-5 #96707d` |

Agent dostaje kwadrat 22px w swoim przygaszonym kolorze, z inicjałem w `--ink`.
Stan agenta jest **słowem** w kolorze nasyconym, nigdy kolorem kwadratu.

> Kolory tożsamości są **nasze** i nie mają odpowiednika w domu: tamtejsze `--graph-*` są
> nasycone, bo obsługują legendę grafu. Reguła powstała przy budowie makiety: referencyjny
> redesign poprzedniego prototypu dawał agentowi Forge dokładnie ten sam kod barwy co „wymaga uwagi".
> Na jednym ekranie oznaczały dwie różne rzeczy tym samym kolorem.

### Aliasy poprzedniej palety

`--accent-edge`, `--accent-wash`, `--attend-wash`, `--fail-wash`, `--human-wash` **żyją**
i wskazują na nowe nazwy. Trzy powierzchnie wołają je jeszcze bezpośrednio i migrują osobno;
nazwa skasowana pod niezmigrowanym komponentem zostawia element bez ani jednej reguły CSS —
awarię, która nie rzuca wyjątku i nie pojawia się w żadnym logu (niezmiennik 25).
Znikają razem z ostatnim wołającym.

Zakazane: gradienty dekoracyjne, drugi kolor marki, kolor jako ozdoba, barwione szkło,
piąty kolor stanu, cień pod czymś, co nie pływa.

---

## 4. Typografia

### Dwie rodziny, jedna reguła

- **Hanken Grotesk** — język ludzki. Zdania, etykiety, przyciski, nagłówki, opisy, logotyp.
- **JetBrains Mono** — wartości maszynowe. Ścieżki, identyfikatory, hashe, liczby, czas, nazwy
  plików, komendy.

**To jest reguła semantyczna, nie estetyczna.** Mono znaczy „to wyprodukowała maszyna i możesz
to skopiować". Widzisz mono → wiesz, że to fakt, nie opis.

Oba kroje są **zmiennymi plikami `.woff2` w repo** (`src/styles/fonts/`), te same, które niesie
`../meetnotes/src/assets/fonts/`. Oba OFL.

> Zamknięty defekt, dla pamięci. Do 2026-08-19 ten dokument żądał Intera, `theme.css` go
> deklarował, a w drzewie nie było ani jednego pliku kroju i ani jednej reguły `@font-face`.
> Aplikacja rysowała się krojem systemowym — po cichu, bez ani jednego błędu w konsoli.
> Dokładnie jedna rodzina na token stoi w cudzysłowie i dokładnie ta jedna ma swój `@font-face`;
> dalsze człony są systemowe i bez cudzysłowu. Cudzysłów jest obietnicą, że plik leży w drzewie.

### Drabinka

| Token | Rodzina | Rozmiar | Waga | Interlinia | Tracking | Użycie |
|---|---|---|---|---|---|---|
| `--t-title` | ui | 20px | 600 | 1.2 | -0.02em | tytuł ekranu, jeden na widok |
| `--t-heading` | ui | 15px | 600 | 1.3 | -0.01em | nagłówek sekcji, tytuł karty |
| `--t-subhead` | ui | 14px | 600 | 1.3 | 0 | tytuł kafelka na liście |
| `--t-body` | ui | 13px | 400 | 1.5 | 0 | zdania i opisy |
| `--t-ui` | ui | 13px | 600 | 1.2 | 0 | przyciski, aktywne etykiety |
| `--t-note` | ui | 12px | 400 | 1.45 | 0 | drugie zdanie, podpowiedź pod polem |
| `--t-label` | ui | 11px | 600 | 1.2 | 0 | **etykieta pola, zdaniowo** |
| `--t-eyebrow` | ui | 11px | 600 | 1.2 | 0.06em | **nadoczko sekcji, WERSALIKI** |
| `--t-meta` | ui | 11px | 400 | 1.2 | 0 | wartość maszynowa w drugim planie |
| `--t-mono` | mono | 12px | 400 | 1.45 | 0 | wartości maszynowe |
| `--t-mono-strong` | mono | 12px | 700 | 1.2 | 0.06em | identyfikator, nazwa agenta |
| `--t-stream` | mono | 13px | 400 | 1.5 | 0 | linia w widoku pracy |

Waga 500 nie istnieje. Drabinka to 400 / 600 / 700.
Rozmiary poniżej 11px nie istnieją. Jeśli coś nie mieści się w 11px, jest niepotrzebne.

`font-variant-numeric: tabular-nums` obowiązuje wszędzie, gdzie cyfry stoją w kolumnie.

### Dwa stopnie tam, gdzie był jeden

`--t-label` i `--t-eyebrow` mają ten sam rozmiar i tę samą wagę, a różnią się jedną rzeczą:
**wersalikami**. Do 2026-08-19 istniał jeden stopień i obsługiwał oba zastosowania, więc
wersaliki wchodziły albo na **każdą** etykietę pola, albo na żadną.

Podział jest sprawdzalny i jest sprawdzany:

- **Nadoczko sekcji** — `AGENTS`, `BUILD`, `WHAT IT DOES`. Wersaliki, tracking `0.06em`.
- **Etykieta pola i rola agenta** — `Name`, `Give up after`, `writes code`. Zdaniowo, tracking 0.

Wersaliki na każdej etykiecie pola są najczęstszym ruchem domyślnego panelu admina i pierwszą
rzeczą, po której formularz przestaje wyglądać jak macOS.

Reguła `text-transform` mieszka **w definicji stopnia**, w warstwie `components`, a nie
w komponentach: Tailwind pozwala tokenowi `--text-*` nieść interlinię, rozstrzelenie i wagę,
ale nie `text-transform`. Warstwa `components` stoi niżej niż `utilities`, więc `normal-case`
nadal wygrywa tam, gdzie makieta nie ma wersalików.

---

## 5. Przestrzeń i kształt

Baza 4px. Skala: `4 · 8 · 12 · 16 · 24 · 32 · 48`.

```
--s-1: 4px    --s-2: 8px    --s-3: 12px   --s-4: 16px
--s-5: 24px   --s-6: 32px   --s-7: 48px
```

- Padding wiersza strumienia: `8px 16px`
- Padding karty: `12px`
- Padding panelu: `16px`
- Odstęp między sekcjami: `24px`

**`border-radius: 2px`. Wszędzie. Bez wyjątków.** Kółka tylko dla kropek stanu (`50%`, 8px).

Wysokość kontrolek: `28px` kompaktowa · `32px` domyślna · `36px` podstawowy przycisk.
Cel dotykowy nie dotyczy — to aplikacja desktopowa sterowana myszą i klawiaturą.

---

## 6. Komponenty

Definicja komponentu = tokeny, nie hexy. Poniżej pełna lista v1.

### `button-primary`
`background: --accent` · `color: --bg` · `--t-ui` · `radius 2px` · `padding 8px 16px` · `height 36px`
Active: `transform: scale(0.98)`. Focus: `outline: 2px solid --accent; outline-offset: 2px`.

### `button-secondary`
`background: --raised` · `color: --ink` · `border: 1px solid --line-strong` · `--t-ui` · `height 32px`

### `button-quiet`
`background: transparent` · `color: --body` · `border: 1px solid --line` · `--t-ui` · `height 28px`

### `button-danger`
Jak `button-secondary`, ale `border: 1px solid --fail-edge` · `color: --fail`. Bez wypełnienia — akcja
niszcząca nie ma być najbardziej rzucającym się w oczy elementem, ma być rozpoznawalna.

### `chip`
`padding 2px 8px` · `--t-label` · `height 20px` · `border 1px solid {stan}-edge` · `background {stan}-wash` · `color {stan}`
Wariant neutralny: `--line` / `--raised` / `--muted`.

### `field`
`background: --well` · `border: 1px solid --line-strong` · `color: --ink` · `--t-mono` · `padding 8px 10px` · `height 32px`
Focus: `border-color: --accent`. Etykieta nad polem w `--t-label` / `--muted`.

### `stream-line` — wiersz w widoku pracy
Siatka `88px 1fr auto`. Padding `8px 16px`. Bez obramowania między wierszami — separacja przez odstęp.
Kolumna 1: nazwa agenta, `--t-mono-strong`, kolor agenta.
Kolumna 2: co robi, `--t-stream`, `--ink`.
Kolumna 3: liczba/czas, `--t-mono`, `--muted`.
Wiersz aktywny: lewy pasek `inset 2px 0 --accent`. Wiersz skończony: `opacity 0.55`.

### `history-line` — zwinięty krok
Siatka `20px 1fr auto`. Padding `6px 16px`. `--t-body`. Znacznik `✓` w `--muted`.
Klik rozwija pełny zapis kroku pod spodem.

### `agent-card` — kafelek w prawej szynie
`background: --panel` · `border: 1px solid --line` · `padding 12px`.
Aktywny: `border-color: --line-strong`, lewy pasek `inset 2px 0 {kolor agenta}`.
Zawiera: kwadrat 14px w kolorze agenta, nazwę (`--t-mono-strong`), rolę (`--t-label`, `--muted`),
jedno zdanie o tym, co robi (`--t-body`), chip stanu.
**Maksymalnie cztery linie tekstu.** Piąta linia to błąd projektowy.

### `node-card` — kafelek w edytorze workflow
Szerokość `280px` · `background: --raised` · `border: 1px solid --line-strong` · `padding 12px`.
Nagłówek: uchwyt `⠿`, nazwa (`--t-heading`), kropka koloru agenta, nazwa agenta (`--t-mono`), `✕`.
Ciało: jedno zdanie po ludzku o tym, co ten krok robi.
Stopka: `◂ po: {krok}` po lewej, `działa przed ▸` po prawej.
Zaznaczony: `border-color: --accent`.

### `loadout-strip` — pasek loadoutu
Patrz §2. Blok `32×8px`, odstęp `8px`, etykieta pod blokiem `--t-label` / `--muted`.

### `modal`
`background: --panel` · `border: 1px solid --line-strong` · `max-width 640px` · `padding 24px`.
Tło za modalem: `rgba(6,9,11,0.72)`. Bez rozmycia, bez animacji wjazdu poza `opacity 120ms`.

### `empty-state`
Wyśrodkowane. Znak `◇` w ramce `1px dashed --line-strong`, zdanie w `--ink`,
jedno zdanie instrukcji w `--muted`, jeden przycisk podstawowy.
**Pusty ekran to zaproszenie do działania, nie komunikat o braku danych.**

---

## 7. Ruch

Jedna mikrointerakcja w całym systemie: `transform: scale(0.98)` na wciśnięciu.

Poza tym:
- Zmiana treści w strefie TERAZ: bez animacji. Tekst po prostu jest inny. Animowanie przepisania
  linii sprawia, że oko goni ruch zamiast czytać.
- Wejście nowej linii historii: `opacity 0 → 1` w `120ms ease-out`. Bez przesunięcia.
- Pulsowanie: **tylko** kropka aktywnego agenta, `opacity 1 → 0.35`, `1.4s steps(2)`.
  Skokowo, nie płynnie — płynne pulsowanie czyta się jak oddychanie i rozprasza.

`@media (prefers-reduced-motion: reduce)` wyłącza wszystko powyżej.

---

## 8. Język interfejsu

**UI jest po angielsku** (decyzja D5). Czasownik w trybie rozkazującym, zdanie proste, bez żargonu.
Wiążąca jest tabela żargon→prosty-język z `docs/research/projects/00-SYNTHESIS.md` §2.2 (55 wierszy).

| Zamiast | Piszemy |
|---|---|
| Submit / Execute workflow | `Run` |
| Configuration | `Settings` |
| Initialize | `Create` |
| Terminate | `Stop` |
| Failed with exit code 1 | `Tests failed · 3 of 40` |
| No records found | `Nothing here yet. Type /plan to start.` |
| tool call, tool_use | *(nic — nazwij czynność: `Read`, `Edited`, `Ran`)* |
| stdout / stderr / exit code | `output`, `didn't work` |
| token, context window, compaction | `length`, `started a fresh page` |
| PTY, session, process, spawn | `terminal`, `agent`, `started` |
| orchestrator / DAG / node | `lead agent`, `workflow`, `step` |
| thinking / reasoning tokens | `Thinking…` |
| diff / hunk | `changes` |

Nazwa akcji nie zmienia się w trakcie przepływu: przycisk `Publish` → komunikat `Published`.

Błąd nie przeprasza i nie jest ogólny. Mówi, co się stało i co z tym zrobić:
> `Can't find claude on your system. Install Claude Code, or point Loadout at it in Settings.`

Pusty ekran to zaproszenie, nie komunikat o braku danych:
> `Nothing here yet. Type /plan to start.`

---

## 9. Kontrola jakości

Komponent nie trafia do repo, dopóki:

- [ ] nie ma w kodzie ani jednego literału hex, px rozmiaru czcionki ani `border-radius` innego niż token
- [ ] focus jest widoczny z klawiatury i wygląda tak samo we wszystkich kontrolkach
- [ ] wygląda poprawnie przy szerokości okna 1100px (najwęższe wspierane)
- [ ] nie używa `--accent` do niczego poza „teraz" i interakcją
- [ ] nie dodaje piątego koloru semantycznego
- [ ] `prefers-reduced-motion` go nie psuje
- [ ] żaden tekst w nim nie jest w mono, jeśli nie jest wartością maszynową
