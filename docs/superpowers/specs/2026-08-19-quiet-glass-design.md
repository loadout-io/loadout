# Redesign: Loadout Quiet Glass

**Data:** 2026-08-19 · **Status:** spec do recenzji człowieka · **Decyzja:** podjęta przez Jakuba w tej sesji

Makiety, na których ta decyzja stanęła:

- trzy kierunki, wariant 3 jest wybrany — <https://claude.ai/code/artifact/838676f4-4779-4bcd-a677-65a8553701f3>
- znak, ikona, logotyp — <https://claude.ai/code/artifact/a691b47b-d32f-40f1-a504-ec7f90e996d0>

---

## 0. Czym ten plik jest i kiedy umiera

To **dokument przejściowy**. Żyje dokładnie tak długo, jak trwa fala zadań opisana w §8,
i **jest kasowany w commicie lądowania ostatniego z nich.**

Powód jest w `CLAUDE.md`: „dwa źródła prawdy to awaria, której to repo unika z premedytacją,
a przy dwóch kopiach zawsze czytasz tę nieaktualną". Po fali prawdą o wyglądzie są cztery
rzeczy i tylko one:

| Plik | Czym jest |
|---|---|
| `docs/DECISIONS-LOCKED.md` D1 | kierunek, którego nie podważa się w planach |
| `docs/design/DESIGN.md` | system projektowy, źródło |
| `src/styles/theme.css` | lustro DESIGN.md, porównywane mechanicznie |
| `docs/mockup/index.html` | wyrocznia układu, parsowana przez testy |
| `docs/branding/` | znak, ikona, logotyp |

**Nie cytuj tego pliku po zakończeniu fali.** Jeśli po niej istnieje — to błąd sprzątania,
nie źródło.

---

## 1. Decyzja i jej granice

Wybrany kierunek: **Loadout Quiet Glass**. Tokeny są **1:1 z domu**
(`../meetnotes/src/design-tokens/`), zastosowane przy naszej gęstości i z naszym słownikiem stanów.
Reguła tamtego systemu brzmi dosłownie *„QUIET GLASS: glass is CHROME, content is paper"* i jest
identyczna z tezą, do której wariant doszedł osobno. Spójność między dwiema aplikacjami w jednym
Docku jest celem, nie efektem ubocznym.

### Co ten redesign zmienia

Paletę, oba kroje, promienie, materiał (szkło na chrome), powłokę okna, formy komponentów,
znak, ikonę i logotyp.

### Czego ten redesign NIE rusza — i to jest zamknięta lista

Te rzeczy są w tym repo wywalczone i redesign nie ma prawa ich osłabić:

- **Sufit gęstości z `docs/ARCHITECTURE.md` §7.** Liczby zostają bez zmian. Zdarzyło się już
  w tej sesji, że projekt chciał 100 px chrome przy suficie 96 — wygrał sufit (§5.3).
- **Cztery stany i ani jeden więcej** (`--live` `--attend` `--fail` `--human`). Piąty stan to
  nowa decyzja człowieka, nie dopisek.
- **Rozdział tożsamości od stanu po nasyceniu.** Kolor agenta jest przygaszony, stan jest
  nasycony, a stan agenta jest **słowem**, nigdy kolorem kwadratu.
- **Rzecz skończona jest cicha.** Ukończony krok to `✓` w `--muted`.
- **Dwie prostopadłe osie nawigacji** (menu = „co robię", karty = „w którym folderze").
- **Jeden fakt, jedno miejsce** (niezmiennik 13). Limit żywych regionów na fakt to 1.
- **Zero żargonu w tekście widocznym dla użytkownika** (niezmiennik 14, decyzja D5).
- **UI nie rysuje relacji, których nie ma w danych** (niezmiennik 17).
- **Kontrolka bez handlera nie wchodzi do repo** (niezmiennik 16).

---

## 2. Nowa D1 — tekst do wklejenia w `docs/DECISIONS-LOCKED.md`

Zastępuje całą obecną sekcję „D1 — Wygląd". **Nie ląduje, dopóki człowiek nie potwierdzi tego
akapitu** — `AGENTS.md` §7 wymienia ten plik wprost.

> ## D1 — Wygląd: Loadout Quiet Glass
>
> *Zrewidowane 2026-08-19. Pierwotna D1 (paleta z `redesign poprzedniego prototypu.dc.html`, `border-radius: 2px`,
> Inter, akcent `#6ee0b0`) jest **cofnięta w całości**. Powody, wszystkie zmierzone: paleta
> mint-na-czerni jest statystyczną średnią tego, co generują modele; `radius: 2px` jest antytezą
> macOS, o który prosiliśmy w tej samej decyzji; a Inter był zadeklarowany w `theme.css` od
> pierwszego dnia i **nie istniał w drzewie** — aplikacja przez cały ten czas rysowała się krojem
> systemowym, po cichu.*
>
> Baza wizualna: **system projektowy meetnotes** (`../meetnotes/src/design-tokens/`, nazwa własna
> „Quiet Glass"). Bierzemy **wartości**, nie inspirację: powierzchnie, obrysy, akcent, promienie,
> cienie, krzywe ruchu, przepis na szkło i oba kroje są 1:1. Dwie nasze aplikacje mają w Docku
> wyglądać na rodzeństwo.
>
> Reguła nadrzędna, wprost z tamtego systemu: **szkło jest chrome, treść jest papierem.**
> Szkło nie wchodzi nigdy pod tekst ani pod kod, które człowiek ma przeczytać.
>
> Co jest nasze, bo dom tego nie rozwiązuje:
>
> - **Gęstość.** listy meetnotes są znacznie luźniejsze — wiersz spotkania zajmuje w ich
>   podglądzie kilkukrotność naszego wiersza strumienia. Te same tokeny, ciaśniejsze zastosowanie.
> - **Rozdział „interaktywne" od „teraz".** `--accent #6e76ff` mówi wyłącznie „to jest
>   interaktywne". `--live #ff7a5c` mówi wyłącznie „to się dzieje w tej chwili". Do 2026-08-19
>   jeden token robił obie prace naraz.
> - **Przygaszone kolory tożsamości agentów.** Domowe `--graph-*` są nasycone, bo obsługują
>   legendę grafu; u nas kolor agenta obok koloru stanu jest awarią.
> - **Znak.** Najmniejszy prawdziwy graf: jedno wejście, dwie równoległe gałęzie, jedna synteza.
>
> Akcent: `#6e76ff`. „Teraz": `#ff7a5c`. Pełna specyfikacja: `docs/design/DESIGN.md`.

---

## 3. Tokeny

Staje się `docs/design/DESIGN.md` §3 i `src/styles/theme.css`. `checks/quick-tokens.sh`
porównuje oba pliki w obie strony: token obecny tylko w jednym z nich jest czerwienią.

### 3.1 Powierzchnie

| Token | Wartość | Użycie | Z domu |
|---|---|---|---|
| `--bg` | `#07070b` | tło aplikacji, kartka treści | `--surface-base` |
| `--panel` | `rgba(255,255,255,.045)` | wypełnienie szkła: menu, pasek, szyna | `--surface-raised` |
| `--raised` | `rgba(255,255,255,.045)` | karty na szkle | `--surface-raised` |
| `--well` | `rgba(255,255,255,.035)` | pola wejściowe, bloki kodu | `--surface-input` |
| `--overlay` | `#1b1b24` | **nieprzejrzyste** menu i podpowiedzi | `--surface-overlay` |
| `--solid` | `#111118` | gdy potrzebna jest prawdziwie krycąca powierzchnia | `--surface-solid` |
| `--hover` | `rgba(255,255,255,.06)` | podkład wiersza pod kursorem | `--surface-hover` |
| `--scrim` | `rgba(0,0,0,.5)` | przygaszenie za modalem | `--scrim` |

### 3.2 Tekst

| Token | Wartość | Użycie |
|---|---|---|
| `--ink` | `#f6f6fa` | nagłówki, wartości, aktywna treść |
| `--body` | `#a6a6b6` | zdania, opisy, treść domyślna |
| `--muted` | `#8a8a9c` | etykiety, metadane, rzeczy skończone |

Dom ma **cztery** stopnie tekstu; bierzemy trzy. `--muted` to domowy `--text-tertiary`, nie
`--text-muted` — i to jest świadome: tamten stopień (`#6c6c7d`) mierzy 3,62:1 na powierzchni
podniesionej, czyli **pod progiem czytelności**, i dom trzyma go wyłącznie dla ≥13 px. U nas
prawie każdy przygaszony napis to metadana ≤12 px, więc jaśniejszy stopień (5,50:1) jest jedynym
poprawnym, a czwartego nie wprowadzamy, żeby nikt nie miał czym sięgnąć po ciemniejszy.

### 3.3 Linie

| Token | Wartość |
|---|---|
| `--line` | `rgba(255,255,255,.09)` |
| `--line-strong` | `rgba(255,255,255,.16)` |
| `--line-subtle` | `rgba(255,255,255,.055)` |

### 3.4 Stan — cztery, plus akcent, który stanem nie jest

| Token | Wartość | Znaczy | Pytanie |
|---|---|---|---|
| `--live` | `#ff7a5c` | **teraz** | co się dzieje w tej chwili? |
| `--attend` | `#f5b14c` | **ty** | co czeka na moją decyzję? |
| `--fail` | `#ff6b6b` | **zepsute** | co poszło źle? |
| `--human` | `#9d7bff` | **człowiek** | co zrobiła osoba, nie maszyna? |
| `--accent` | `#6e76ff` | *to jest interaktywne* | — nie odpowiada na pytanie o stan |

Warianty: `--accent-hover #8a90ff` · `--accent-active #5b63f0` · `--accent-soft rgba(110,118,255,.16)`
· `--accent-ring rgba(110,118,255,.5)` · `--live-soft rgba(255,122,92,.16)`
· `--live-edge rgba(255,122,92,.5)` · `--attend-soft rgba(245,177,76,.14)`
· `--attend-edge rgba(245,177,76,.5)` · `--fail-soft rgba(255,107,107,.14)`
· `--fail-edge rgba(255,107,107,.5)` · `--human-soft rgba(157,123,255,.14)`
· `--human-edge rgba(157,123,255,.5)`

#### Ryzyko, które trzeba zmierzyć, a nie ocenić okiem

`--live #ff7a5c` i `--fail #ff6b6b` różnią się odcieniem o **~13°**. W meetnotes nigdy nie stoją
obok siebie; w naszym strumieniu stoją w sąsiednich wierszach. Reguła, która to rozstrzyga:

> **`--live` i `--fail` nigdy nie dzielą formy.**
> `--live` występuje wyłącznie jako: podkład aktywnego wiersza strefy „teraz", jego obrys,
> aktywny segment paska loadoutu, pulsująca kropka, kropka na karcie w tle.
> `--fail` występuje wyłącznie jako: glif `✕`, obrys chipa, lewa krawędź bloku błędu.

Kryterium akceptacji jest **statyczne, na źródle** — i to jest wymuszone, nie wybrane:
to repo nie ma `jsdom` ani `environment` w `vite.config.ts`, więc vitest biegnie w node, testy
renderują przez `renderToStaticMarkup`, a **obliczonego stylu nie ma skąd wziąć**. Kryterium
oparte na `getComputedStyle` byłoby niewykonalne.

Wykonalny kształt: test czyta źródła komponentów jako tekst (wzorzec, którym już działa
`shell-matches-mockup.test.tsx`) i zbiera dwa zbiory — nazwy klas występujące na elementach
niosących token `live-*` i te na elementach niosących `fail-*`. Test pada, jeśli zbiory się
przecinają, i pada też wtedy, gdy któryś z nich jest **pusty** (kontrola przeciw pustemu
porównaniu: parser, który cicho nic nie dopasował, przeszedłby na niczym).

Świadomie NIE w `checks/`: `AGENTS.md` §7 wymaga tam zgody człowieka, a to sprawdzenie jest
kryterium jednego zadania, nie strażnikiem całego repo.

### 3.5 Tożsamość agenta — nasze, przygaszone

`--id-1 #6f8496` · `--id-2 #7f7597` · `--id-3 #94886b` · `--id-4 #6b9285` · `--id-5 #96707d`

Zbliżona jasność, niskie nasycenie. Agent dostaje kwadrat 22 px w swoim kolorze z inicjałem
w `--ink`. **Stan agenta jest słowem w kolorze nasyconym, nigdy kolorem kwadratu.**

### 3.6 Promienie, odstępy, cienie, ruch

```
--radius-sm: 9px      --radius-md: 13px     --radius-lg: 18px     --radius-pill: 999px
--space-1: 4px  --space-2: 8px  --space-3: 12px  --space-4: 16px
--space-5: 24px --space-6: 32px --space-7: 48px
--shadow-sm: 0 1px 2px rgba(0,0,0,.4)
--shadow-md: 0 12px 32px rgba(0,0,0,.35)
--shadow-lg: 0 22px 56px rgba(0,0,0,.48)
--transition: 200ms cubic-bezier(.32,.72,0,1)
--transition-fast: 130ms cubic-bezier(.32,.72,0,1)
```

Pasmo promieni domu to `9 / 13 / 18 / 24 / pill`. **Bierzemy dolny koniec: 24 px nie istnieje
w Loadoucie.** Narzędzie o tej gęstości przy 24 px wygląda jak aplikacja na iPada. Wiersze
strumienia nie mają promienia wcale — to one utrzymują gęstość.

**Cień wyłącznie pod tym, co PŁYWA**: panel nawigacji, modal, podpowiedź, menu. Element wewnątrz
strony nie ma cienia nigdy. Domowy `--ease-spring` **nie wchodzi** — nie mamy ani jednej
powierzchni, która wjeżdża.

### 3.7 Szkło i aurora

```
--glass-blur: 30px            --glass-saturate: 135%
--glass-border: rgba(255,255,255,.10)
--glass-highlight: inset 0 1px 0 rgba(255,255,255,.10)

--shell-glass-bg: linear-gradient(160deg,
    rgba(255,255,255,.075) 0%, rgba(255,255,255,.035) 55%, rgba(255,255,255,.055) 100%)
--shell-glass-border: rgba(255,255,255,.11)
--shell-glass-inner: inset 0 1px 0 rgba(255,255,255,.18)
--shell-glass-blur: 36px
--shell-active-bg: rgba(255,255,255,.09)
--shell-active-icon: #8a90ff

--aurora-field:
    radial-gradient(40% 46% at 4% 8%,  rgba(110,118,255,.15), transparent 58%),
    radial-gradient(38% 42% at 3% 94%, rgba(157,123,255,.10), transparent 60%)
```

**Aurora mieszka WEWNĄTRZ okna** — statyczna winieta przy lewej krawędzi, pod panelem nawigacji.
To rozwiązanie domu i ma konsekwencję, którą trzeba powiedzieć wprost, bo oszczędza całą klasę
pracy: **szkło ma co załamywać bez dotykania strony Rusta.** Żadnego `transparent: true`, żadnego
`windowEffects`, żadnego ryzyka migotania WKWebView, żadnej zależności od tapety użytkownika.
Kolumna czytania siedzi na czystym `--bg`.

`@media (prefers-reduced-transparency: reduce)` zamienia wszystkie powierzchnie szklane na
`--solid`. To wymóg HIG i dom go egzekwuje; my kopiujemy zachowanie, ale **bez suwaka
przejrzystości** — nie mamy powierzchni Settings, w której by mieszkał.

### 3.8 Luka w lustrze, którą trzeba domknąć — wymaga zgody człowieka

`checks/quick-tokens.sh` porównuje DESIGN.md z theme.css wzorcem `#[0-9a-fA-F]{6}`. **Ten wzorzec
nie widzi `rgba(...)`.** W starej palecie wszystkie 21 tokenów było heksami, więc luka nie miała
znaczenia. W Quiet Glass **większość powierzchni i wszystkie obrysy to biel-alfa**, czyli lustro
przestaje pilnować ponad połowy palety, meldując przy tym zielono i podając liczbę „N colour
tokens agree" — dokładnie ta awaria, którą ten plik sam o sobie opisuje w nagłówku.

Zmiana jest addytywna: dołożyć do `TOKEN_HEX` alternatywę na `rgba()` i do wzorca theme.css to samo.
`AGENTS.md` §7 wymaga na `checks/` zgody człowieka, więc to jest **prośba, nie zadanie**.
Bez niej redesign da się dowieźć, ale lustro zostaje częściowo slepe i trzeba to zapisać
w `docs/HARNESS-QUEUE.md` jako świadomy dług.

---

## 4. Typografia

```
--font-ui:   "Hanken Grotesk", -apple-system, BlinkMacSystemFont, system-ui, sans-serif
--font-mono: "JetBrains Mono", "SF Mono", ui-monospace, Menlo, monospace
```

**Oba kroje wchodzą do repo jako `.woff2` z `@font-face`.** Tauri nie ma sieci, a to domyka
defekt, który `theme.css` sam o sobie pisze od pierwszego dnia. Licencje: Hanken Grotesk — OFL,
JetBrains Mono — OFL. Kryterium na to jest mechaniczne i nie ufa deklaracji:
plik istnieje na dysku **i** `@font-face` go wskazuje **i** nazwa z `@font-face` jest pierwszym
członem `--font-ui`.

Reguła semantyczna zostaje bez zmian: **mono znaczy „to wyprodukowała maszyna i możesz to
skopiować"**. Inter jest zastąpiony, reguła nie.

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
| `--t-mono` | mono | 12px | 400 | 1.45 | 0 | wartości maszynowe |
| `--t-mono-strong` | mono | 12px | 700 | 1.2 | 0.06em | identyfikator, nazwa agenta |
| `--t-stream` | mono | 13px | 400 | 1.5 | 0 | linia w widoku pracy |

Waga 500 nie istnieje: `400 / 600 / 700`. Poniżej 11 px nie istnieje.

**`--t-label` rozszczepia się na dwa stopnie i to jest zmiana merytoryczna.** Do 2026-08-19 jeden
token obsługiwał i nadoczko sekcji („Agents", „Build"), i etykietę pola („Name") — więc wersaliki
z rozstrzeleniem `.08em` wchodziły albo wszędzie, albo nigdzie. Dom rozdziela: nadoczka mają
wersaliki, etykiety pól są zdaniowe. Wersaliki na każdej etykiecie pola to najczęstszy ruch
domyślnego panelu admina i pierwsza rzecz, po której formularz przestaje wyglądać na macOS.

`font-variant-numeric: tabular-nums` obowiązuje wszędzie, gdzie stoją cyfry w kolumnie.

---

## 5. Powłoka

### 5.1 Rama

```
okno (radius 14, macOS rysuje róg)
└─ padding 6, aurora + --bg
   ├─ panel nawigacji  208px · radius --radius-lg · szkło powłoki · PŁYWA (cień)
   └─ kartka treści    radius --radius-md · nieprzejrzysta · obrys --line
      ├─ karty         32px  · szkło · kapsuły
      ├─ pasek         52px  · szkło
      └─ praca         strumień + strefa „teraz" + wiersz wejścia | szyna agentów
```

Panel nawigacji jest **jedyną rzeczą w aplikacji, która pływa**, więc jest jedyną, która ma
cień. Szyna agentów jest szkłem, ale leży na kartce treści — nie pływa, nie ma cienia.

### 5.2 Aktywny wiersz nawigacji

Neutralne „szkło na szkle": `--shell-active-bg`, etykieta w `--ink`, **akcent bierze wyłącznie
glif** (`--shell-active-icon`). To reguła domu wprost z `glass.css`: *„the accent never fills
chrome, it colors the active glyph/label only"*. Która sekcja jest otwarta, jest powiedziane
dokładnie raz — przez `aria-current`, z którego bierze się wygląd (niezmiennik 13, tak jak dziś).

### 5.3 Budżet chrome — i to on wygrał z projektem

Sufit z `ARCHITECTURE §7` to **96 px nad pierwszą treścią**. Naiwna wersja pływającego panelu
dawała `9 (odstęp) + 1 (obrys kartki) + 34 (karty) + 56 (pasek) = 100`. Ten sam paragraf mówi:
*„Każdy kolejny pasek nad treścią wymaga usunięcia innego, nie negocjacji limitu"*.

Wersja, która wchodzi: `6 + 1 + 32 + 52 = **91**`. Pięć pikseli zapasu.

`CHROME_INSET_TOP` (dziś **44**, nowe **38**) i `trafficLightPosition.y` (16) są **związane i mierzone
razem** — światła macOS pływają nad panelem, a marka zaczyna się pod nimi:
`16 + 20 (wysokość świateł) + 8 (odstęp) − 6 (odstęp okna) = 38`. Zmiana jednej z tych liczb bez
drugiej jest czerwienią w kryterium okna; osobno każda wygląda rozsądnie.

### 5.4 Ikony nawigacji

Pięć glifów 16 px, obrys 1.6, `currentColor`. To jest cały system ikon i ma gramatykę:

| Sekcja | Glif | Dlaczego |
|---|---|---|
| Run | trójkąt | jedyna rzecz, która się dzieje |
| Workflows | trzy węzły i dwie krawędzie | rzecz, która **jest** grafem — ten sam alfabet co znak |
| Agents | dwie zachodzące płyty | zbiór |
| Skills | czterokątna iskra | zbiór zdolności |
| Memory | dwie płyty w stosie | zbiór zapisów |

Węzły i krawędzie **wyłącznie** dla rzeczy, które są grafem. To nie jest estetyka — to ta sama
reguła co niezmiennik 17, przeniesiona na ikonografię.

### 5.5 Ruch

Bez zmian wobec dzisiejszej dyscypliny, z podmienioną krzywą na domową:

- `transform: scale(.98)` na wciśnięciu — jedyna mikrointerakcja.
- Strefa „teraz" **nie animuje** zmiany treści. Tekst po prostu jest inny.
- Nowa linia historii: `opacity 0 → 1` w `--transition-fast`.
- Pulsuje **wyłącznie** kropka pracującego agenta: `opacity 1 → .35`, `1.4s steps(2)`.
  Kropka gotowości w stopce **przestaje pulsować** — nie jest „teraz", jest dostępnością,
  a regionów animujących się od jednego zdarzenia mamy sufit 2.
- `prefers-reduced-motion: reduce` wyłącza wszystko powyżej.

---

## 6. Komponenty — różnice wobec dzisiejszego DESIGN §6

Nie przepisuję całej listy; poniżej wyłącznie to, co się zmienia.

| Komponent | Zmiana |
|---|---|
| `button-primary` | `--accent`, tekst `--ink`, `--radius-sm`, wysokość 32. Wypełnienie **jednolite** — HIG: prominentny przycisk nigdy nie jest gradientem |
| `button-secondary` | `--raised` + `--line-strong`, `--radius-sm` |
| `button-quiet` | przezroczysty + `--line`, wysokość 28 |
| `button-danger` | obrys `--fail-edge`, tekst `--fail`, bez wypełnienia |
| `chip` | `--radius-pill`, obrys `{stan}-edge`, tło `{stan}-soft` |
| `field` | `--well` + `--line-strong`, `--radius-sm`, focus `--accent-ring` |
| `stream-line` | bez promienia, bez tła. Aktywny: podkład `--live-soft` + obrys `--live-edge`, `--radius-pill` |
| `agent-card` | `--raised` + `--line`, `--radius-sm`, `--glass-highlight`. **Maksymalnie cztery linie tekstu** — bez zmian |
| `node-card` | `--radius-md`, zaznaczony: obrys `--accent` |
| `loadout-strip` | segmenty w **jednym szklanym torku** (`--radius-pill`). Skończony `--line-strong`, aktywny `--live`, czekający obrys `--line` |
| `modal` | `--overlay` (nieprzejrzysty!), `--radius-lg`, `--shadow-lg`, tło `--scrim` |
| `nav-item` | nowy: glif + etykieta + opcjonalna plakietka; aktywny wg §5.2 |
| `mark` | nowy: znak jako komponent. Węzły klasą `fill-body`, krawędzie `stroke-line-strong` — **nie** `currentColor`, bo to dwa różne tokeny. Wariant jednobarwny (pasek menu, favicon) bierze `currentColor` |

**Ograniczenie, które trzeba znać, zanim napiszesz komponent:** `checks/quick-tokens.sh` odrzuca
w `src/**` każdy literał hex, każdą liczbową wartość `font-size` i `border-radius` bez `var(`,
oraz każdą arbitralną wartość Tailwinda (`text-[13px]`, `rounded-[9px]`, `bg-[#07070b]`,
`stroke-[…]`, `fill-[…]`). Dlatego **znak w `src/ui/brand/mark.tsx` używa `currentColor`
i klas tokenowych, a gradientowa wersja ikony NIE MOŻE być komponentem** — jej miejsce jest
w `docs/branding/*.svg` i `src-tauri/icons/*`, czyli poza `src/`.

---

## 7. Marka

Pełna konstrukcja z liczbami jest w artefakcie brandingowym; tutaj to, co wiążące.

### 7.1 Znak

Najmniejszy **prawdziwy** graf: jedno wejście, dwie równoległe gałęzie, jedna synteza.
Siatka 24 × 24. Węzły: `3,7·12` · `12·5,1` · `12·18,9` · `20,3·12`. Promień 1,95; węzeł syntezy
2,15 (+10%, bo z wielu wychodzi jedno). Krawędź 1,25, zakończenia okrągłe — stosunek średnicy
węzła do grubości linii **3,1 : 1**, i to jest liczba, na której ten znak stoi: przy 2,4 : 1 węzeł
czyta się jako zgrubienie linii i cały znak zamyka się w pierścień (zmierzone na 176 px).
Sylwetka: romb 16,6 × 13,8 — **szerszy niż wysoki**, bo graf płynie w poziomie.

Dlaczego to, a nie cztery kwadraty, które są znakiem dzisiaj: cztery luźne kwadraty nie mają
krawędzi, więc nie mają relacji, więc nie są grafem. Nowy znak dokłada dokładnie dwie rzeczy —
**krawędzie i kierunek** — i są to jedyne dwie rzeczy z pięciu z D6, których żaden vendor nie
zbuduje, bo nie ma w tym interesu.

W chrome znak jest **neutralny**: węzły `--body`, krawędzie `--line-strong`. Coral w znaku byłby
kłamstwem wtedy, kiedy nic nie chodzi.

### 7.2 Ikona macOS

Przepis 1:1 z `../meetnotes/docs/branding/murmur-icon.svg`, żeby dwie aplikacje w jednym Docku
wyglądały na rodzeństwo: squircle `rx=232` na płótnie 1024, radialne tło
`#221f52 → #100f26 → #07070f` (cx 50%, cy 36%, r 78%), sheen `#ffffff` 10% → 0 na 34% wysokości,
temat wyśrodkowany z poświatą `--accent` 55%, ostra krawędź wewnętrzna `#ffffff` 10% grubości 3
przy `inset 1.5`.

**Trzy rozmiary są osobnymi rysunkami, nie skalowaniem:**

| Rozmiar | Co się zmienia |
|---|---|
| 1024 / 512 / 256 / 128 | rysunek pełny |
| 32 | krawędzie grubsze o 30%, węzły bez gradientu, bez sheenu, bez krawędzi wewnętrznej |
| 16 | tylko sylwetka rombu i cztery kropki, jedna barwa |

To nie jest perfekcjonizm: przy 32 px cztery krawędzie po 38 jednostek zlewają się w plamę,
i widać to wprost w artefakcie. `.icns` jest **zestawem**, nie skalowaniem.

### 7.3 Logotyp

`loadout` — **małymi literami, zawsze**, Hanken Grotesk 600, tracking −0,035 em, `--ink`,
nigdy w akcencie. Odstęp znak↔słowo: 1/3 wysokości znaku. Pole ochronne: połowa wysokości znaku.
Podpis: **many agents, one plan** — Hanken 500, `--muted`.

Dzisiejsze `LOADOUT` w mono z rozstrzeleniem `.12em` wypada: mono w tym systemie znaczy „to
wyprodukowała maszyna", a nazwa produktu jest językiem ludzkim. Dom pisze `murmur` dokładnie tak.

Podpis nie dokłada ani jednego terminu do słownika — `agent` i `plan` już są w interfejsie
(decyzja D5, niezmiennik 14).

**Logotyp idzie do repo jako krzywe, nie jako tekst z `font-family`.** Powód jest zmierzony w tym
repo: deklaracja wskazująca na krój, którego nie ma, daje po cichu krój zapasowy — i właśnie tak
przez cały czas działał Inter. Logotyp złożony tekstem powtórzyłby ten błąd tam, gdzie widać go
najbardziej.

### 7.4 Pliki

| Plik | Co to jest |
|---|---|
| `docs/branding/loadout-icon.svg` | ikona 1024, źródło dla całego `.icns` |
| `docs/branding/loadout-icon-32.svg` | ręcznie dociągnięta wersja 32 px |
| `docs/branding/loadout-icon-16.svg` | sylwetka i cztery kropki, jedna barwa |
| `docs/branding/loadout-logo.svg` | lockup poziomy, tekst jako krzywe |
| `docs/branding/loadout-mark.svg` | sam znak, jednobarwny, `currentColor` |
| `src-tauri/icons/*` | 32, 128, 128@2x, `icon.icns`, `icon.png` |
| `src/ui/brand/mark.tsx` | znak jako komponent, dwa tokeny, zero literałów |

---

## 8. Co się psuje i w jakiej kolejności to ląduje

### 8.1 Wyrocznie, które trzeba przeprowadzić

`docs/mockup/index.html` **zmienia się pierwsza**, bo dziewiętnaście plików testowych parsuje ją
jako wyrocznię w tym samym biegu testu:

```
src/ui/shell/shell-matches-mockup.test.tsx      src/sections/run/run-matches-mockup.test.tsx
src/ui/shell/type-ladder.test.ts                src/sections/run/entry-row.test.tsx
src/ui/shell/nav-furniture.test.tsx             src/sections/run/strip/strip.test.ts
src/ui/shell/window.test.tsx                    src/sections/run/session/layout.test.ts
src/ui/shell/workspace-switcher.test.tsx        src/sections/run/session/agent-screen-is-reachable.test.tsx
src/sections/agents/agent-form.test.tsx         src/sections/run/feed/line-says-who-and-how-much.test.tsx
src/sections/agents/library-is-reachable.test.tsx  src/sections/run/tabs-switch-workspaces.test.tsx
src/sections/memory/mounted.test.tsx            src/sections/skills/mounted.test.tsx
src/sections/memory/passed-row.test.tsx         src/sections/workflows/list/tile.test.tsx
src/sections/workflows/step-panel/overrides.test.tsx
```

**Reguła, która czyni tę falę tanią:** gdzie tylko można, makieta zachowuje **kształt selektora
i właściwości**, które wyrocznie parsują, a zmienia wyłącznie **wartość**. Wtedy test przechodzi
sam, bo czyta oczekiwaną wartość z pliku. Przykład: `.app { grid-template-columns: 208px
minmax(0,1fr) }` zostaje tą samą regułą o tej samej właściwości.

Gdzie zmiana kształtu jest nieunikniona — `--r: 2px` w makiecie oraz `--radius-sq: 2px`
i `--radius-dot: 9999px` w `theme.css` giną na rzecz `--radius-sm/md/lg/pill`, rozszczepienie
`--t-label`, dojście `--live` — zadanie **musi** zaktualizować parser wyroczni i powiedzieć to
wprost w nagłówku kryterium. Cicha zmiana parsera pod testem jest gorsza niż czerwień.

### 8.2 Kolejność zadań

Jedna gałąź naraz, pełna bramka po każdej (`./integrate.sh`). Każde zadanie ma `## AC-n`
i jedną linię `check:` ze ścieżką do pliku testu, globalnie unikalną.

| # | Zadanie | Zależy od | Rdzeń |
|---|---|---|---|
| 1 | **Tokeny i kroje** — DESIGN.md §3–§4 przepisana, theme.css jako jej lustro, dwa `.woff2` z `@font-face` | — | `quick-tokens` zielony w obie strony; kryterium na krój nie ufa deklaracji |
| 2 | **Makieta** — `docs/mockup/index.html` w Quiet Glass, kształty selektorów zachowane | 1 | wyrocznia, która CZYTA wartość z makiety, przechodzi sama; pada tylko ta, która ma wartość wpisaną z palca — i wtedy naprawą jest odczyt, nigdy przepisanie liczby |
| 3 | **Powłoka** — pływający panel, ikony, karty-kapsuły, budżet chrome 91 | 2 | `CHROME_INSET_TOP` i `trafficLightPosition` mierzone razem |
| 4 | **Znak i ikona** — `docs/branding/*`, `src-tauri/icons/*`, `mark.tsx` | 1 | trzy rysunki ikony, nie jeden przeskalowany; zero literałów w `src/` |
| 5 | **Strumień i strefa „teraz"** — `--live` na aktywnym wierszu, rozłączność form `--live`/`--fail` | 3 | sprawdzenie rozłączności form z §3.4 |
| 6 | **Formularze i inspektory** — pięć sekcji, `--t-label` zdaniowo, `--t-eyebrow` w nadoczkach | 3 | zero wersalików na etykietach pól, zmierzone |
| 7 | **Nowa D1** — `docs/DECISIONS-LOCKED.md` | 1–6 zielone | **ląduje ostatnie i tylko za zgodą człowieka**; ten plik ginie w tym samym commicie |

Zadanie 7 jest ostatnie celowo: dopóki nie jest zielone, D1 opisuje stan, który naprawdę stoi
w trunku. Decyzja zablokowana, która wyprzedza kod, jest deklaracją, nie decyzją.

---

## 9. Kontrola jakości — zastępuje DESIGN §9

Komponent nie trafia do repo, dopóki:

- [ ] nie ma w `src/` ani jednego literału hex, rozmiaru czcionki, promienia ani arbitralnej
      wartości Tailwinda
- [ ] focus jest widoczny z klawiatury i wygląda tak samo we wszystkich kontrolkach
      (`--accent-ring`)
- [ ] wygląda poprawnie przy szerokości okna 1100 px
- [ ] `--accent` nie oznacza w nim nic poza „to jest interaktywne"
- [ ] `--live` i `--fail` nie dzielą w nim formy
- [ ] nie dodaje piątego koloru stanu
- [ ] szkło nie leży pod tekstem ani pod kodem
- [ ] `prefers-reduced-motion` i `prefers-reduced-transparency` go nie psują
- [ ] żaden tekst w nim nie jest w mono, jeśli nie jest wartością maszynową
- [ ] etykieta pola nie jest w wersalikach
- [ ] nie ma cienia, jeśli nie pływa

---

## 10. Poza zakresem

| Rzecz | Dlaczego nie teraz |
|---|---|
| Jasny motyw | tokeny są ustawione tak, że jasny zestaw jest **dosypaniem jednego bloku**, ale go nie budujemy — to osobna decyzja o produkcie, nie o wyglądzie |
| Suwak przejrzystości | dom ma go w Settings; my nie mamy powierzchni Settings wśród pięciu sekcji |
| `windowEffects` / przezroczyste okno | aurora wewnątrz okna daje szkłu co załamywać bez dotykania Rusta (§3.7) |
| Prawdziwe PTY | odłożone decyzją D4, redesign tego nie zmienia |
| `--ease-spring` | nie mamy powierzchni, która wjeżdża |
| Zmiana liczb w `ARCHITECTURE §7` | to one wygrały z projektem i tak ma zostać |
