# Plan: kontekst, pętle, learningi — faza 6

2026-08-23 · analiza + plan wykonawczy · czytaj po `docs/PLAN.md`, przed `tasks/T-86.md`

Ten dokument ma dwóch czytelników: **człowieka**, który decyduje o czterech pytaniach z §6,
i **orchestratora `/build`**, który ma przejechać zadania T-86…T-97 w kolejności z §4
bez pytania o nic, czego nie ma w tym pliku. Prawdą o każdym zadaniu jest `tasks/<ID>.md`;
tutaj jest to, czego z pojedynczego zadania nie widać: dlaczego, w jakiej kolejności,
co można puścić równolegle i gdzie są pułapki.

---

## 1. Diagnoza w sześciu zdaniach

1. Silnik (DAG, procesy, dowody śmierci, pliki-jako-prawda, `reconcile`) jest poprawny
   i zmierzony; planista nie zna ani jednej nazwy etapu. **Tego nie ruszamy.**
2. Warstwa nad nim stoi na **niewypowiedzianym kontrakcie**: Loadout oczekuje od agenta
   konkretnych rzeczy (ostatnia wypowiedź = przekazanie, trzy sekcje, 8 KB, `outcome:`
   u sędziego, `klucz: wartość` do tras) i do 2026-08-23 nie mówił mu żadnej z nich.
3. Model kontekstu jest **bezstanowy i jednokrokowy**: krok widzi wyłącznie ścieżki przekazań
   bezpośrednich poprzedników. W łańcuchu to działa; w pętli `max_turns` runda k+1 nie widzi
   własnej rundy k, sędzia nie widzi swojego poprzedniego werdyktu, a pętla, która **przeszła**,
   znika z fan-inu (`NOT_NEEDED` nie oddaje przekazania).
4. Pętla uczenia ma **czytnik bez producenta**: blok `What you know` jest wpięty, budżety
   i promocja działają, ale nic nigdy nie tworzy kandydata — `~/.loadout/memory/` nie istnieje
   po 23 biegach, `inherit` dostaje zawsze pusty wybór.
5. Co najmniej osiem kontrolek nie ma skutku (`copies`, `thinking`, `vendorOptions`,
   `writeResultsTo`, `handover`, kafelek `check`, „Run this step again", panel „What X was
   given") — złamany niezmiennik 16, i to w miejscach, które człowiek klika codziennie.
6. Zmierzone: **23 biegi, 0 `succeeded`**; 96-minutowy bieg za ~$40 u Claude'a padł na
   kontrakcie, nie na modelu. To jest koszt punktów 2–4, nie silnika.

Pełna analiza z odwołaniami do linii jest w historii tej rozmowy; ten plik zachowuje
wyłącznie to, co przekłada się na zadania.

---

## 2. Mapa: każde znalezisko ma adres

| # | Znalezisko | Gdzie ląduje |
|---|---|---|
| F1 | Agent nie wie, że jego odpowiedź jest przekazaniem; marnuje tury na próby zapisu plików | **T-86** AC-1 |
| F2 | Agent nie zna swojego limitu czasu | **T-86** AC-2 |
| F3 | `repaired`/`truncated` przekazania idą tylko do `debug!` | **T-86** AC-3 |
| F4 | Runda k+1 pętli nie widzi własnej rundy k ani wejścia pętli | **T-87** AC-1 |
| F5 | Sędzia nie widzi swoich poprzednich werdyktów | **T-87** AC-2 |
| F6 | Pętla, która przeszła, nie oddaje nic do fan-inu (`NOT_NEEDED`) | **T-87** AC-3 |
| F7 | Indeks przekazań nie mówi, czym jest każdy plik (własna runda / werdykt / wejście) | **T-87** AC-4 |
| F8 | Krok, który padł z `carry-on`, nie oddaje nic; trzy ścieżki porażki omijają `when_this_one_fails` | **T-87** AC-5 |
| F9 | „Pick up here" kopiuje przekazania, ale żadne nie trafia do promptu | **T-88** |
| F10 | Kafelek `check` (jedyny „co się stało") nie ma przycisku ani panelu | **T-89** |
| F11 | `copies` nie rozwija się; `{{copy}}` nigdzie nie podstawiane | **T-90** AC-1 |
| F12 | `vendorOptions` (przelotka D6) nie dociera do argv | **T-90** AC-2 |
| F13 | `writeResultsTo` ma zero czytelników | **T-90** AC-3 |
| F14 | `handover: {fields}` ma zero czytelników | **T-90** AC-4 |
| F15 | `thinking` (quick…deepest) nie trafia do żadnego vendora | **T-91** |
| F16 | Brak producenta learningów (refleksja po biegu z T6 §5.3) | **T-92** AC-1, AC-2 |
| F17 | Brak komendy tworzenia/odrzucenia notatki; mockup ma „Discard" bez handlera | **T-92** AC-3, AC-6 |
| F18 | `last_used_at` nigdy nie aktualizowane → LRU to porządek po id | **T-92** AC-4 |
| F19 | Auto-pamięć Claude Code biegnie w krokach Loadouta do katalogu użytkownika; lever z T6 §10.4 (`--settings autoMemoryDirectory`) nieużyty; `with_settings` bez wołającego | **T-92** AC-5 |
| F20 | `inherit` zawsze pusty — `RunRequest` nie ma pola na wybór | **T-93** |
| F21 | Pula slotów per bieg, nie per aplikacja; `Registry` bez wołającego | **T-94** AC-1 |
| F22 | Brak limitu kosztu; `--max-budget-usd` istnieje w CLI (S-2 rozstrzygnięte) i jest nieużyte | **T-94** AC-2, AC-3, AC-5 |
| F23 | `Weight::Heavy` tylko w teście; krok `check` bierze zwykły slot | **T-94** AC-4 |
| F24 | Gałęzie i worktree zostają po biegu na zawsze | **T-95** AC-1, AC-2 |
| F25 | `same-copy` nie jest sądzone przez sprawdzenie kolizji | **T-95** AC-3 |
| F26 | Panel sesji podaje `handoffs: []`, `notes: []` na sztywno | **T-96** AC-1 |
| F27 | „Run this step again" nigdy się nie renderuje | **T-96** AC-2 |
| F28 | Kroki Codeksa pokazują tylko prozę (`decode_codex` nie istnieje) | **T-97** AC-1 |
| F29 | Codex `look-only` + `reachesTheWeb` = zero sieci, bez słowa w UI | **T-97** AC-2 |
| F30 | `tools: only [...]` u agenta Codex potrafi odmówić biegu o listę, której driver nie używa | **T-97** AC-3 |
| F31 | Codex nie raportuje kosztu ani tokenów | **T-97** AC-4 |
| F32 | Lead (czat) nie dostaje połączeń MCP u żadnego vendora | **T-97** AC-5 |
| F33 | Nieaktualne komentarze: `run.rs:119` (tee), `claude.rs:55`, `history.rs:176`, `ipc.rs:604`, `engine/mod.rs` „SZKIELET", `drivers/mod.rs:34` | naprawia zadanie, które ma dany plik w OWNS (wpisane w „Sprzątanie po drodze" każdego kontraktu) |
| F34 | `docs/ARCHITECTURE.md` opisuje `INDEX.md`, `memory/agents/`, globalny semafor, 200 linii/25 KB, „dwa rodzaje kafelka" | **§5 tego planu** — orchestrator po ostatnim lądowaniu, bez zadania (wyjątek właściciela, `AGENTS.md` §2) |
| F35 | Pamięć per projekt (`<repo>/.loadout/memory`) obiecana, niezaimplementowana | **§6, decyzja 1** |
| F36 | `supersede()` (korekty przekazań) bez wołającego; `Kind` zawsze `findings` | **§6, decyzja 2** |
| F37 | Klucz API Linear leży w `~/.loadout/triggers/*.json` jawnym tekstem (0600) | **§6, decyzja 3** — poza zakresem tej fazy |
| F38 | Importowany agent `backend-dev.md` ma ciało zdublowane (pełne + skrócone) | **§6, decyzja 4** — do zmierzenia w imporcie, nie w tej fazie |

Nic z listy nie zostaje bez adresu. Jeśli orchestrator znajdzie znalezisko spoza listy —
zapisuje je w `docs/STATUS.md` i **nie rozszerza** żadnego zadania.

---

## 3. Zadania

| ID | Tytuł | Zależy od | Dotyka `commands/run.rs` | Kryteriów |
|---|---|---|---|---|
| T-86 | Każdy krok agenta wie, jak oddać wynik i ile ma czasu | T-80 (w trunku) | tak | 3 |
| T-87 | Pętla pamięta swoje rundy, fan-in dostaje to, co przeszło | T-86 | tak | 5 |
| T-88 | „Pick up here" niesie przekazania poprzedniego biegu | T-87 | tak | 3 |
| T-89 | Kafelek „sprawdź" da się postawić i ustawić z płótna | — | nie | 4 |
| T-90 | Cztery martwe pola kroku dostają skutek | T-88 | tak | 5 |
| T-91 | Poziom myślenia dociera do obu vendorów | T-90 | tak | 3 |
| T-92 | Learningi mają producenta | T-91 | tak | 6 |
| T-93 | Dziedziczenie z repo gospodarza ma nośnik | T-92 | tak | 3 |
| T-94 | Jedna pula na aplikację, budżet biegu, ciężki slot | T-93 | tak | 5 |
| T-95 | Po biegu nie zostają kopie ani gałęzie bez pracy | T-94 | tak | 3 |
| T-96 | Sesja agenta mówi prawdę o tym, co dostał | T-88 | nie | 2 |
| T-97 | Codex na równi z Claude'em | T-95 | tak | 5 |

Kryteria trzymają się pasma z `AGENTS.md`; T-96 ma dwa, bo trzecie wymagałoby środowiska DOM,
którego repo nie ma (decyzja z T-85: `e2e/`, nie nowa zależność).

---

## 4. Kolejność i równoległość

`commands/run.rs` jest w OWNS dziesięciu zadań z dwunastu. To nie jest wada planu — to jest
miejsce, w którym mieszka kontekst kroku — ale znaczy, że te dziewięć **ląduje szeregowo**,
w podanej kolejności, bo każde następne zakłada poprzednie w trunku.

```
łańcuch A (run.rs):   T-86 → T-87 → T-88 → T-90 → T-91 → T-92 → T-93 → T-94 → T-95 → T-97
obok, bez run.rs:     T-89 (od razu)      T-96 (po T-88)
```

Fale dla `/build` (§4 tego pliku zastępuje §4 z `.claude/commands/build.md` dla tej fazy):

| Fala | Równolegle | Warunek startu |
|---|---|---|
| 1 | **T-86**, **T-89** | trunk zielony |
| 2 | **T-87** | T-86 w trunku |
| 3 | **T-88** | T-87 w trunku |
| 4 | **T-90**, **T-96** | T-88 w trunku |
| 5 | **T-91** | T-90 w trunku |
| 6 | **T-92** | T-91 w trunku |
| 7 | **T-93** | T-92 w trunku |
| 8 | **T-94** | T-93 w trunku |
| 9 | **T-95** | T-94 w trunku |
| 10 | **T-97** | T-95 w trunku |

Lądowanie zawsze pojedynczo (`integrate.sh`, pełna bramka po każdej gałęzi), jak dotąd.

### Protokół dla orchestratora

1. Przeczytaj `docs/STATUS.md`, potem ten plik, potem `tasks/T-86.md`. Nie czytaj raportów
   z `docs/research/`.
2. Dla każdej fali: `./ship-task.sh <ID> --agent claude --reviewer codex` per zadanie, równolegle
   tyle, ile ma fala (Workflow `parallel`). Codex ma kredyty od 2026-08-20 — cross-vendor jest
   domyślny (D3). `LOADOUT_CARGO_LOCK_WAIT=2400` przy fali szerszej niż jedno zadanie rustowe.
3. Po fali: `./integrate.sh <gałąź>` po jednej. Czerwony trunk po merge'u = stop, diagnoza,
   zapis w `docs/STATUS.md`, człowiek.
4. Kod `1` z `ship-task.sh`: czytaj `runs/<ID>/` i **powód**. Jeśli kryterium da się spełnić
   tylko plikiem spoza OWNS — to jest wynik, nie przeszkoda: zapisz w `docs/STATUS.md`
   „T-xx ZAMKNIĘTE: …", nie rozszerzaj OWNS, nie łataj kontraktu. Pięć takich przypadków z tej
   fazy jest przewidzianych w §7.
5. Kod `2`: harness, nie zadanie. Stój.
6. Po ostatnim lądowaniu wykonaj §5 (dokumentacja) i dopisz do `docs/STATUS.md` akapit
   z licznikami: ile zadań, ile rund naprawczych, ile zamknięć z §7, koszt z `runs/build-loop.tsv`.

---

## 5. Dokumentacja po fazie (bez zadania, wyjątek właściciela)

Po T-97 w trunku orchestrator nanosi w `docs/ARCHITECTURE.md`, bez zmiany numeracji sekcji:

- §4: argv `claude` — dopisać `--effort`, `--settings` (auto-pamięć per bieg), `--max-budget-usd`;
  wykreślić „tee do `logs/`" jako przyszłość — robi to `evidence.rs` od T-34.
- §6a: „jeden semafor na całą aplikację" — od T-94 prawda; dopisać, że do T-94 był per bieg.
- §6b: `INDEX.md` przekazań — zastąpić opisem indeksu w prompcie z etykietami ról (T-87);
  „liczba rodzajów kafelka zostaje dwa" → trzy (D6 z 2026-08-20) plus `serve` jako rodzaj
  sterownika.
- §8: `memory/INDEX.md` i `memory/agents/<slug>.md` — wykreślić (zakres w front-matterze,
  nie w katalogu: `memory/notes.rs`); `<repo>/.loadout/memory/` — zależnie od decyzji 1 z §6;
  dopisać `attachments/` i `mem/` w katalogu biegu.
- §5: tabela stanów — dopisać `FailedAndCarriedOn`, `Route::Blocked`, `settle_leftovers`;
  wykreślić `attempt += 1` do czasu, aż ponowienie istnieje.
- §11: S-2 rozstrzygnięte pomiarem 2026-08-23: `--max-turns`, `--max-budget-usd` i `--effort`
  istnieją w `claude` 2.1.241 (`claude --help`).

---

## 6. Cztery decyzje, które musi podjąć człowiek (nie blokują fali 1–4)

1. **Pamięć per projekt.** ARCHITECTURE §8 obiecuje `<repo>/.loadout/memory/`; kod trzyma
   wszystko w `~/.loadout/memory/`. Albo T-92 dostaje AC „notatka `this-project` leży pod
   korzeniem projektu", albo §8 zostaje poprawione. **Domyślnie (jeśli brak decyzji do fali 5):
   zostaje globalnie, §8 poprawiamy.** Powód: jeden korzeń = jeden skan; rozdział na projekty
   wymaga drugiego korzenia w `what_the_agents_know` i w ekranie Pamięć.
2. **Korekty przekazań.** `supersede()` i sześć rodzajów `Kind` nie mają wołającego. Usunąć
   (mniej kodu) czy wpiąć (`/correct` w wierszu wejścia)? **Domyślnie: zostaje jak jest**, bo nie
   kosztuje biegu; wraca przy pierwszej skardze.
3. **Klucz API Linear jawnym tekstem.** Keychain to osobne zadanie z własnym researchem.
   Poza fazą 6.
4. **Zdublowane ciało importowanego agenta.** Sprawdzić na drugim imporcie, czy to model
   (translate) czy `apply`. Poza fazą 6, ale wpisać do STATUS przy okazji.

---

## 7. Pięć pułapek, które ta faza zna z góry

1. **`RunSpec` nie ma `Default` i ma 31 miejsc konstrukcji.** Każde nowe pole (T-91 `effort`,
   T-94 budżet) wchodzi **szwem addytywnym** (`with_*` na konkretnym typie albo pole
   `Option` z `#[serde(default)]` ustawiane po konstrukcji), nigdy nowym polem literału.
   Inaczej `quick-scope` świeci czerwono na 31 plikach spoza OWNS.
2. **Słownictwo.** `quick-vocabulary` skanuje tekst widoczny dla użytkownika **i komunikaty
   asercji**. W UI i w `expect(..., 'reason')` nie ma: `handoff`, `verdict`, `judge`, `loop`,
   `session`, `gate`, `node`, `DAG`. Są: „what it passed on", „the tester", „way back",
   „try N of M", „checks". Tabela: `FOUNDATIONS.md` §2.2.
3. **`before` musi być czerwone na asercji.** Rust: sygnatura z `todo!()`, plik w
   `tests/it/` **plus** linia `mod` w `tests/it/main.rs` (OWNS każdego zadania rustowego).
   TS: szkielet, który się importuje i pada na `expect`.
4. **Nowa komenda IPC = wiersz w lustrze.** `src/sections/commands-wired.test.ts` sądzi cały
   eksport; każde zadanie dokładające komendę ma ten plik w OWNS (T-92, T-94, T-95).
5. **Nowa `pub fn` bez wołającego pali `quick-wired`.** Szew dla testu ma mieć produkcyjnego
   wołającego w tym samym zadaniu, albo być `pub(crate)`.

Przewidziane zamknięcia „stój i zgłoś": T-89 AC-4 (e2e może wymagać `e2e/harness.ts`),
T-92 AC-5 (fabryka oddaje `Arc<dyn AgentDriver>`, a `with_settings` żyje na typie — jeśli
szew `AgentDriver::with_settings` z domyślnym `None` nie wystarczy, to jest zgłoszenie),
T-94 AC-1 (jeśli `RunDeps` nie da się rozszerzyć bez `lib.rs` — jest w OWNS),
T-97 AC-1 (fixture `codex-stream.jsonl` może nie zawierać wszystkich rodzajów zdarzeń),
T-97 AC-4 (cennik Codeksa nie jest znany — kryterium prosi o tokeny, nie o dolary).
