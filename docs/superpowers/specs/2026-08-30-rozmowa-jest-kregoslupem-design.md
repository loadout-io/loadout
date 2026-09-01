# Rozmowa jest kręgosłupem, bieg jest blokiem w środku

**Skarga właściciela, 2026-08-30, ze zrzutu ekranu biegu `20260830-191440`:**
„nie podoba mi się ta ściana tekstu, ciężko się to czyta, czy możemy popracować nad UI tego
terminala". I zaraz potem, po pierwszej propozycji: „to rozmowy z agentami, a co z nadrzędnym
agentem? jak to sensownie pokazywać".

Drugie zdanie jest ważniejsze od pierwszego i to ono wyznacza ten projekt.

---

## 1. Co jest naprawdę zepsute

### 1.1 Odpowiedź agenta nie ma ciała, więc wlała się w wiersz

Kurator biegu skleja prozę do jednej linii i **ma to robić** — to jest reguła 1 architektury,
rozwinięta w sekcji 4a. Wada jest gdzie indziej: odpowiedź agenta nie ma drogi „za wiersz",
której ta reguła wymaga, więc całe 78 linii wylądowało w tekście wiersza.

Ta podsekcja została napisana PRZED tamtym znaleziskiem i twierdziła, że wadą jest samo
sklejanie. To było błędne; pełny dowód stoi w sekcji 4a.

### 1.2 Okno nie wie, kto jest liderem

Rust stempluje linie lidera napisem `agent: "Lead"` (`commands/chat.rs`, `const LEAD`). To zwykły
napis, tej samej klasy co `"Frontend"`. W całym `src/sections/run/feed/` nie ma ani jednego
miejsca, które by to słowo znało.

Konsekwencja: w jednej kolumnie, **w jednym i tym samym kształcie wiersza**, leżą trzy różne
rzeczy — zdania człowieka do lidera, tury lidera i jego własna praca, oraz praca kroków biegu.

### 1.3 Zamknięta lista rodzajów wiersza zna wyłącznie pracę

`ran`, `read`, `edit`, `search`, `memory`, `done`, `problem`, `thinking`, `note`, `asked`,
`suggested`. Nie ma w niej pojęcia „tura w rozmowie". Ten feed został zaprojektowany jako
**dziennik pracy**, a rozmowa z liderem została do niego wlana bez pytania modelu o zdanie.

### 1.4 Makieta mówi rzeczy, których apka nie robi

`docs/mockup/index.html` zawiera `.ln.note{max-width:64ch}` (czytelna miara wiersza),
`.ln.collapsed` i `.ln .exp` (stan zwinięty i kontrolka rozwijająca). Żadnej z tych trzech rzeczy
w oknie nie ma. **Kryterium `run-matches-mockup.test.tsx` sądzi dokładnie dwie reguły** —
`.work { grid-template-columns }` i `.feedcol { grid-template-rows }` — więc reszta makiety
odjechała bez ani jednej czerwieni.

---

## 2. Czego ten projekt NIE wymyśla

**Kształt docelowy jest już zablokowany decyzją D4** (`docs/DECISIONS-LOCKED.md`), dosłownie:

```
❯ /plan zbuduj parser CSV

  ✓ plan gotowy · 4 kroki        [rozwiń]

  Forge   pisze  src/parser.rs
  Needle  testy  12 ✓  0 ✗
  Rivet   czeka  na Needle

❯ _
```

> „To jest docelowa gęstość informacji. Jeśli widok robi się gęstszy niż to — jest źle."
> „Domyślnie zwinięte: wszystko poza tym, co się właśnie dzieje i co wymaga uwagi."

Dzisiejszy ekran robi **dokładne przeciwieństwo** drugiego zdania: każda odpowiedź kroku jest
rozwinięta, więc jedna z nich zasłania cały bieg. Ten projekt nie jest nowym kierunkiem — jest
domknięciem D4.

---

## 3. Kształt docelowy

```
  Ty      Zrób research i odpal test-workflow

  Lead    Sprawdziłem repo — Angular 20, zoneless. Trzy workflow
          pasują; test-workflow ma 9 kroków. Odpalam na murmur-1.

          ▾ test-workflow · krok 5 z 9
            ✓ Reasearch A      56 tur · 12 min
            ✓ Final Plan        1 tura · 7 min
            ✓ Frontend         81 tur · 15 min
              ⌄ ANSWER · 78 wierszy
            ● Backend          pracuje…
            · Combine          czeka

  Lead    Frontend skończył. Rail działa w obu motywach,
          485 testów zielonych.
```

Trzy reguły, z których to wynika:

1. **Tura rozmowy jest papierem.** Pełna szerokość czytelnej miary (64ch z makiety), markdown,
   **nigdy zwijana**. To jest rzecz, po którą człowiek przyszedł; schowanie jej za kliknięciem
   byłoby schowaniem odpowiedzi na jego własne pytanie.
2. **Praca jest kompaktowa i domyślnie zwinięta** (D4). Dotyczy tak samo pracy kroków, jak
   **własnej pracy lidera** — jego wywołania powłoki są pracą, nie rozmową, i składają się pod
   jego turą.
3. **Bieg jest jedną rzeczą, nie dwustoma wierszami.** Blok wewnątrz tury, którą go odpalono.

---

## 4. Decyzje wiążące, które ten projekt musi uszanować

| Reguła | Co z niej wynika tutaj |
|---|---|
| Niezmiennik 15 — kuracja w Ruście, nie w CSS | „Ten wiersz jest rozmową, a ten pracą" **musi** rozstrzygnąć Rust. Okno zgadujące po nazwie agenta jest widokiem, który da się zepsuć arkuszem stylów. |
| Niezmiennik 17 — UI nie rysuje relacji, których nie ma w danych | Bieg jako blok wymaga **prawdziwego pola** „do którego biegu należy ten wiersz". Grupowanie wymyślone w oknie jest ozdobną krzywą. |
| Niezmiennik 13 — jeden fakt, jedno miejsce | Nazwa lidera nie może być rozpoznawana w oknie po literach. |
| Niezmiennik 14 + D5 | Napisy na ekranie po angielsku, bez żargonu. |
| Niezmiennik 16 | `[rozwiń]` bez handlera nie wchodzi. |
| Niezmiennik 18 | Sufit gęstości jest mierzony. Zwijanie działa na jego korzyść. |
| D1 — szkło jest chrome, treść jest papierem | Pod turą rozmowy nie wchodzi szkło. |
| D6 — nowa flaga to nowe POLE | Rozróżnienie jedzie jako pole linii, nie jako nowy rodzaj kafelka. |

---

## 4a. ZNALEZIONE PO NAPISANIU SEKCJI 5 — to jest niedokończona droga, nie wada stylu

Pierwsza wersja tego projektu zakładała, że trzeba **odwrócić sklejanie** w biegu. To było błędne
i sprzeczne z architekturą. `engine/line.rs` zapisuje regułę 1 wprost:

> „jedna czynność, jeden wiersz; **treść siedzi ZA wierszem, nigdy w nim**. Dlatego `Line::text`
> nie zawiera `\n`, a wszystko, co ma ciało, jedzie przez `detail` i `detail_id`."

Sklejanie w biegu jest więc **tą regułą**, nie niedoróbką. Ściana powstała z czegoś innego.

### Co jest zbudowane

| Kawałek | Stan |
|---|---|
| Tabela `events` (`run_id`, `kind`, `level ∈ headline\|detail\|raw`, `body`), append-only z triggerami | **działa**, zapisuje przy każdym biegu (`workspace.rs`, pompa) |
| `detail_id` mintowany przez kuratora (`Curator::minted`) | **działa**, jedzie drutem do `HistoryRow.detailId` |
| `Line::Handoff { agent, text }` — wariant na odpowiedź agenta | **istnieje w enumie** |

### Czego nie ma

| Brak | Dowód |
|---|---|
| Ktokolwiek czytający `events` z powrotem | zero `SELECT … FROM events` w całym `src-tauri/src/`; jedyne wystąpienie to `INSERT` |
| Czytelnik `detailId` w oknie | pole dojeżdża do `HistoryRow` i kończy się tam; `feed/line.tsx` go nie tyka |
| Producent `Line::Handoff` | zero `Line::Handoff {` poza `line.rs`; komentarz enumu przyznaje to wprost („konstruuje je planista") |

### Wniosek

Odpowiedź agenta wylądowała w **tekście wiersza**, bo droga zaprojektowana dla niej nie została
dokończona. Ten projekt nie przeprojektowuje ekranu — **kończy tę drogę**. Kształt
`⌄ ANSWER · 78 wierszy`, który właściciel wybrał 2026-08-30, to dosłownie `Line::Handoff`
z `detail_id`, którego nikt nie wyprodukował.

**To unieważnia pozycję A z pierwszej wersji sekcji 5.** Bieg dalej skleja prozę do jednej linii
i ma to robić. Zmienia się to, że odpowiedź agenta przestaje być prozą w wierszu, a staje się
wierszem Z CIAŁEM.

---

## 5. Robota, w kolejności

Kolejność jest dobrana tak, żeby każda pozycja była **sama w sobie widoczna** i żeby żadna nie
pogarszała ekranu w drodze do następnej.

| # | Co | Gdzie |
|---|---|---|
| 1 | Proza dostaje czytelną miarę — 64ch, **wartość już stoi w makiecie** i nikt jej nie stosuje | `feed/line.tsx` |
| 2 | Odpowiedź agenta staje się `Line::Handoff` z `detail_id`, a jej ciało idzie do `events` na poziomie `detail` | `engine/line.rs`, `commands/run.rs` |
| 3 | Rust umie oddać ciało po `detail_id` | nowa komenda + `SELECT` w `store` |
| 4 | Wiersz odpowiedzi renderuje się jako `ANSWER · N wierszy` i otwiera ciało na klik | `feed/model.ts`, `feed/line.tsx` |
| 5 | Ciało renderuje markdown | nowa zależność + reguła w `deny.toml` |
| 6 | Tura lidera zostaje prozą w wierszu — bez zmian w Ruście, `talking()` już to robi — i dostaje to samo traktowanie typograficzne | `feed/line.tsx` |
| 7 | Bieg jest zwijanym blokiem z krokami w środku | `feed/`, makieta |
| 8 | Makieta dostaje kształt bloku biegu | `docs/mockup/index.html` |

**Pozycja 5 jest decyzją właściciela z 2026-08-30**: wybrał bibliotekę markdown, znając koszt.
`src/ui/shell/permissions.test.ts` zostaje w mocy bez zmian — webview nie dostaje ani `shell:`,
ani `fs:`, a `BANNED_CRATES` zostaje puste po stronie Rusta.

**Czego pozycja 2 NIE robi:** nie odwraca reguły 1 i nie tyka `keeps_line_breaks`. Bieg dalej
skleja prozę, rozmowa dalej jej nie skleja, i obie te rzeczy są poprawne.

---

## 6. Czego ten projekt świadomie nie rusza

- **Szyna agentów po prawej** (`rail/`) zostaje bez zmian. Odpowiada na inne pytanie — „kto
  w ogóle jest w tym biegu" — i odpowiada na nie dobrze.
- **Pasek kroków u góry** (`strip/`) zostaje. To jest jeden fakt w jednym miejscu i blok biegu
  go nie powtarza: blok mówi, co każdy krok zrobił, pasek mówi, gdzie jest bieg.
- **Sekcja TERAZ** zostaje jako osobna strefa. Ma własne kryteria
  (`now-holds-only-live-work.test.ts`) i własną regułę: wyłącznie praca, która trwa.
