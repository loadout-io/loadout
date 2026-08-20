# Stan budowy — 2026-08-18, 00:40

Ten plik jest **żywy**. Aktualizuje go orchestrator po każdym lądowaniu. Prawdą o zadaniu jest
`tasks/<ID>.md`; tutaj jest wyłącznie to, czego z plików zadań nie widać: co już stoi w trunku,
co stanęło i dlaczego.

## 2026-08-20, 07:10 — biurko rozliczone: trzy zadania w trunku, niezmiennik 29, trzy decyzje w kontraktach

**Wyladowane: T-68, T-69, T-70.** Pelna bramka po kazdym, 15/0. Do tego **niezmiennik 29**
w karcie pracy, **trzy decyzje produktowe** zamienione w kontrakty (T-70, T-71, T-72)
i **T-73 wycofane po pomiarze**.

### Niezmiennik 29 — kryterium asertuje zdanie tam, gdzie czlowiek je widzi

Wszedl na wyrazne polecenie wlasciciela, po tym jak recenzent zlapal te klase CZTERY RAZY na
zielonej bramce w jednej fali. Regula nie zada niemozliwego w repo bez jsdom i mowi to wprost:
czysty modul dowodzi TRESCI, `renderToStaticMarkup` obecnosci na prawdziwej sciezce,
`e2e/harness.ts` dojscia po prawdziwym kliknieciu. Wolno wybrac jedno z trzech; nie wolno
poprzestac na wartosci zwroconej przez funkcje, ktorej nikt nie wola.

**Regula od razu zaczela pracowac.** Recenzent T-70 zlapal, ze kryteria wolaja `Threads::say`
wprost, a **zywa aplikacja `Threads` nie konstruuje w ogole** — `AppState.chat` to nadal
`Mutex<Option<Chat>>`. Biblioteka dla lidera byla wiec dowiedziona na typie, ktorego produkt
nie wola.

### Blokada, ktora postawil orchestrator, i ktora zdejmuje T-71 AC-5

Przyczyna tamtego stanu NIE jest wada pisarza i to jest wazniejsze niz sama naprawa. Pisarz T-60
opisal go co do zdania (`ipc.rs`, „WATEK PER ZAKRES ISTNIEJE I NIE STOI TUTAJ"): `Threads::say`
wymaga wskazanego lidera, wskazania nie ma czym dowiezc z okna, bo wymagaloby klucza obok
`folder` w `io.ts` — a **moj mandat na tamten plik pozwalal dopisac wylacznie `folder`**.
Odmowil podstawienia polowy i mial racje: rozmowa zakladajaca nowy watek przy kazdym zdaniu
bylaby gorsza od tej, ktora stoi.

Blokada jest wiec granica orchestratora, nie modelu, i dlatego zdejmuje ja zadanie, ktore posiada
wszystkie trzy pliki. **Nauka: waski mandat na cudzy plik potrafi zablokowac podpiecie, ktore
jest CALYM sensem zadania. Kiedy go stawiasz, sprawdz, czy zadanie da sie wtedy skonczyc.**

### T-73 wycofane, bo wada byla zamknieta I PILNOWANA

Kontrakt na sklejanie wierszy przechodzacych przez koniec biegu zeszl z „PASSES before
implementation" na obu kryteriach. Zamiast zgadywac, zmierzylem mutacja: zdjecie `groups.clear()`
z `runEnded` zapala `nothing-live-survives-the-run.test.ts > closes the open fold windows, so the
next run cannot grow the last row of this one`; po przywroceniu 7 passed. Czyli pisarz T-68
przewidzial te wade i pokryl ja kryterium **w tym samym biegu**, a recenzent czytal kod, ktory
juz ja zamykal.

**Wzor do zapamietania:** „zielone before" nie odroznia „zachowanie istnieje" od „test jest
zepsuty". Kiedy oba kryteria swieca zielono przed implementacja, mutacja odpowiada w 30 sekund,
a lektura nie odpowiada wcale.

### Trzeci raz: limit konta wyglada jak zly kontrakt

T-72 zeszlo rc=1 z „did not RUN" na wszystkich czterech kryteriach i galezia zawierajaca
**wylacznie commit kontraktowy**. To ten sam podpis, co dwa razy wczesniej tej nocy.
Rozpoznanie jednolinijkowe: `git log main..HEAD` na galezi pokazuje jeden commit zamiast kilku.
Po resecie wznowione bez zmiany ani jednego znaku w kontrakcie.

### Co czeka

| co | stan |
|---|---|
| **T-72** — procesy, ktore Loadout trzyma (`/start`, kafelek w szynie, kill z dowodem) | wznowione |
| **T-71** — plusik otwiera terminal + AC-5 (zywa komenda przez rejestr watkow) | po T-72, dzieli `ipc.rs` i `io.ts` |
| T-40, T-41, T-45, T-56 | starsza kolejka, nietkniete |
| T-64, T-65 | triggery Lineara, druga fala |

**Etap B dla terminali** (biegi rownolegle: tozsamosc biegu na drucie, `stop_run(id)`, rejestr
zamiast jednego `AppState.live`) nie ma jeszcze kontraktu. Jego warunkiem wstepnym byl T-69
i ten juz stoi w trunku.

## 2026-08-20, 05:40 — terminal, lider i siedem zadan w trunku

**Wyladowane: T-58, T-66, T-67, T-60, T-61, T-62, T-63.** Pelna bramka po KAZDYM ladowaniu,
15/0 za kazdym razem; na galeziach przed ladowaniem T-58 20/0, T-60 19/0, T-61 19/0, T-62 18/0,
T-63 19/0, T-66 17/0, T-67 17/0.
**T-59 wycofane w trakcie.** Fala wziela sie z rozmowy z wlascicielem, nie z planu.

### Ladowanie stalo godziny na CUDZEJ niezacommitowanej pracy, i jak zostalo zdjete

`integrate.sh` odmawia lądowania na brudnym drzewie i ma racje. W drzewie glownym leza od
kilku godzin trzy pliki CUDZEJ, niezacommitowanej pracy (`commands/run.rs`,
`memory/handoff.rs`, nowy `tests/handoff_attachment_is_openable.rs` — zalaczniki przekazan).
Rozwiazanie: **zmierzyc, zanim sie ruszy.** `./verify.sh quick` dalo 13/0, a `cargo test
--test handoff_attachment_is_openable` 1 passed — praca byla wiec SKONCZONA i dala sie
zacommitowac jako wlasny commit. Nic nie zginelo: `git reset --soft HEAD~1` cofa ja jednym
ruchem. `git stash` bylby gorszy, bo znika wtedy z drzewa robota, ktorej autor jest w trakcie
zadania.

**I tu wpadla pulapka warta zapisania.** Ta praca przechodzila `quick` (`--lib`) i swoj wlasny
test, a mimo to zostawiala trunk CZERWONY: `full-clippy` sadzi `--all-targets`, czyli takze
`tests/`, i jedno `redundant closure` przy `-D warnings` zatrzymalo cala fale. `integrate.sh`
zameldowal to dokladnie tak, jak trzeba — czerwien na main PRZED jakimkolwiek merge'em, nic nie
wyladowane, zeby wina nie spadla na pierwsza galaz. Naprawa: jedna linia,
`.filter_map(Result::ok)`. **Zielony `quick` plus zielony wlasny test NIE znaczy, ze trunk
przyjmie.**

**Nauka operacyjna:** drugi agent pracowal NA TRUNKU, nie w worktree. Przy dwoch agentach na
jednym repo to zatrzymuje lądowanie calej fali. Kazda praca — takze jego — potrzebuje pliku
zadania z blokiem OWNS, bo blok OWNS jest jedynym zamkiem, jaki to repo ma.

### Stos zamiast czekania — brudny trunk nie musi zatrzymywac budowy

Repo ma na to gotowy mechanizm i tej nocy zostal uzyty pierwszy raz na serio: `FROM=` w
`worktree.sh` odbija galaz od wskazanej bazy, a `LOADOUT_TRUNK=` ustawia zakres, po ktorym
sadzi `quick-scope`. Trzy fale poszly na stosie:

    main -- task-T-58 -- task-T-66 -- task-T-67          (front)
    main -- task-T-60 -+- task-T-61                      (lider)
                       +- task-T-62
                       +- stack-T-63 (T-60+T-61+T-62) -- task-T-63

**Trzy pulapki stosu, kazda zmierzona:**

1. **Worktree z bazy nie widzi plikow zadan zacommitowanych na main.** Kontrakt trzeba najpierw
   domergowac do bazy, inaczej bieg nie ma czego zamrozic.
2. **Rozszerzenie kontraktu wciagniete do galezi merge'em z main wyglada dla bramki jak zapis
   poza zakresem.** `quick-scope` sadzi CALA galaz wzgledem bazy, wiec zmieniony `tasks/<ID>.md`
   jest „plikiem spoza OWNS", choc zmienil go orchestrator. Harness robi to u siebie poprawnie
   (`refresh_harness_from_trunk` przywraca po merge'u wylacznie wlasny plik zadania) — recznym
   merge'em ten krok sie pomija. Poprawna kolejnosc: **baza do trunku, galaz do bazy, plik
   zadania z bazy, dopiero potem `TASK.md`**.
3. **Baza zlozona z dwoch galezi konfliktuje o `TASK.md`** — kazda niesie swoj zamrozony
   kontrakt pod ta sama sciezka. W bazie `TASK.md` musi ZNIKNAC, inaczej swiezy worktree rodzi
   sie w trybie wznowienia i sadzi sie cudzym kontraktem.

### T-59: kontrakt byl zly i wykrylo to dopiero uruchomienie

Mial wpuscic `WebSearch`/`WebFetch` na kazdy szczebel `Policy`, zeby lider do researchu nie
wymagal oddania calej maszyny. Zapowiedziana cena byly dwa napisy w `claude_argv_policy.rs`.
Prawdziwa: `driver_claude_policy_surface.rs:171` trzyma `editing.is_subset(&unlimited) &&
editing != unlimited`, a po przeniesieniu sieci w dol `Unrestricted` nie dokłada do `--tools`
niczego wlasnego — obie listy sie zrownuja. Zmierzone: **401 passed / 3 failed**, czerwien poza
OWNS. Kryterium T-53 jest DOBRE (ostre zawieranie lapie adapter drukujacy jedna liste dla trzech
polityk), wiec bieg zatrzymany, grupa ubita z dowodem ESRCH, specyfikacje (818 linii) zachowane.
Zamiennik — **T-63** — robi to per agent, wiec agent domyslny sklada argv co do bajtu jak dzis
i zaden wyladowany straznik nie przestaje byc prawdziwy.

### Recenzent w SLABSZYM trybie zlapal szesc defektow na ZIELONEJ bramce

Ten sam vendor, inny model, rola recenzenta. Zaden z tych szesciu nie byl widoczny dla
zadnego z moich kryteriow:

1. **Widmowy agent w szynie.** `roster.ts` bije kafelek na kazde odrebne `row.agent`; po T-58
   kazda komenda sklada wiersz podpisany oknem, wiec pierwsze `/stop` sadza agenta „working"
   na zawsze. -> **T-66, zielone.**
2. **Widmowy wiersz w strefie TERAZ.** Ta sama linia idzie do mapy `doing`, a `now.tsx` nie
   bramkuje listy wierszy propsem `live`. -> **T-67, zielone.**
3. **Przypiete pytanie przezywa bieg.** `runEnded` nie gasi `waiting`, wiec karta „Needs your
   answer" wisi po biegu i dalej daje sie kliknac. -> **T-68, napisane.**
4. **Druga tabela `FileAccess` -> `Policy`.** T-60 nie posiadalo `run.rs`, wiec lider dostal
   reczna kopie tabeli, a pisarz ZAPISAL w komentarzu, ze wymog jest niespelniony. -> **T-63 AC-4.**
5. **Przycisk propozycji martwy w aplikacji.** Renderowal sie tylko z propsem `command`, ktorego
   `HistoryRow` nie mial, a produkcyjni wolajacy nie podawali. Kryterium zielone, funkcja
   nieistniejaca. -> naprawione w T-61 po rozszerzeniu OWNS.
6. **Start osieroca agenta z `/ask`.** `begin_a_run` dostalo warunek, `begin_run` nie — a wola
   je Start, `/run` i zielony Run. Osierocony agent pracuje i placi, Stop go nie dosiega.
   Zgloszone niezaleznie przez DWA rozne biegi recenzji. -> **T-69, napisane.**

### Wzor, ktory kosztowal trzy rozszerzenia OWNS

Pisalem bloki OWNS pod pliki, ktore zadanie ZMIENIA, i nie pod **lustra**, ktore o tej zmianie
musza sie dowiedziec. Trzy razy: nowy rodzaj wiersza przewrocil `feed/collapse.test.ts`
(dziewiec rozwinietych), nowy wariant na drucie tablice `KINDS: [LineKind; 16]`, nowa komenda
`commands-wired.test.ts`. Kazde lustro zachowalo sie poprawnie — wymusilo swiadoma decyzje
zamiast przepuscic ja po cichu.

**Regula na nastepne kontrakty:** zadanie dotykajace drutu (nowy rodzaj wiersza, nowa komenda,
nowe pole w `RunSpec`) dostaje swoje lustro w OWNS od razu, z mandatem waskim do jednego wiersza.

Wszystkie trzy rozszerzenia poszly procedura §5c z dowodem mechanicznym: linie `## AC-`,
`check:` i `expect:` porownane miedzy zamrozonym `TASK.md` i nowym kontraktem, za kazdym razem
identyczne co do znaku.

### Limit uzycia konta wyglada jak zly kontrakt

Trzy biegi zeszly naraz z „did not RUN (No test files found)" i galeziami zawierajacymi WYLACZNIE
commit kontraktowy. Bramka nazwala to wada kontraktu, bo nie ma czym odroznic „kontrakt jest zly"
od „agent nigdy nie odpowiedzial". Rozpoznanie: zero plikow specyfikacji na trzech galeziach
jednoczesnie. Po resecie limitu te same kontrakty przeszly bez zmiany ani jednego znaku.
**Wniosek operacyjny:** nie wiecej niz dwie fazy kontraktu naraz.

### `scripts/detach.py` jest w repo

Zginal dwa razy (19.08 i 20.08), za kazdym razem kosztem sesji, ktora go potrzebowala.
Zmierzone tej nocy: dziewiec biegow w czterech falach, zero zgubionych na granicy tury.

### Konflikt przy ladowaniu, ktory byl prawdziwy

`task-T-62` zderzyl sie z `entry/entry.tsx` przepisanym przez T-58: jedno zadanie przebudowalo
wiersz wejscia (historia strzalka, echo do strumienia, ognisko), drugie dolozylo do niego `/ask`.
Trzy hunki, rozwiazane addytywnie z zachowaniem architektury MLODSZEJ, bo ona jest na trunku.

Ostatnia pozostalosc znalazl `tsc`, nie ja: dwa wywolania `setSaid` przezyly merge, bo lezaly
POZA znacznikami konfliktu — T-58 skasowal ten stan, przenoszac odpowiedzi wiersza do strumienia.
Wniosek na przyszlosc: po recznym rozwiazaniu konfliktu w pliku, ktory ktos przepisal, `tsc`
jest tania kontrola przeciw pozostalosciom, ktorych `git` nie pokazal.

Drugi wniosek, tanszy: **kazda galaz stosu nosi swoj `TASK.md`**, a `integrate.sh` kasuje go przy
ladowaniu — wiec druga galaz w kolejce konfliktuje o ten plik. Zdejmuj `TASK.md` z galezi
PRZED ladowaniem, jednym commitem na kazda.

### Co czeka

| co | stan |
|---|---|
| **T-68** — koniec biegu gasi wszystko, co opisywalo zywy bieg (2) | napisane |
| **T-69** — zaden start nie osieroca poprzednika (2) | napisane, niezmiennik 6 |
| T-40, T-41, T-45, T-56 | starsza kolejka, nietkniete |
| T-64, T-65 | triggery Lineara, druga fala; dziela `ipc.rs` z T-60 i T-62 |

**Luka wymieniona, nie zamknieta:** AC-4(c) w T-61 wymaga, zeby zdanie odmowy „wracalo i bylo
pokazane", a testowana jest tylko polowa „wracalo" — bez jsdom `onClick` nie odpala sie w zadnym
tescie. Prawdziwe klikniecie sadzi wylacznie harness e2e (tak zrobilo T-58 AC-5). Ta sama luka
dotyczy `start-invokes.test.tsx` i jest w tym repo strukturalna, nie swieza.

## 2026-08-20, 00:20 — D6 ma trzeci rodzaj kafelka, i to byla decyzja czlowieka

**Wyladowane tej nocy: T-53, T-10, T-54, T-55, T-57.** Pelna bramka po kazdym, 15/0.
Strategia „harness jest nasz, dziedziczymy tekst" stoi w trunku w calosci:
`drivers/{codex,command,host}.rs` i `inherit/{scan,rewrite,wire}.rs`, plus `Step::Check`
w schemacie.

### Blokada, ktora zatrzymala T-55, i jak zostala zdjeta

T-55 skonczylo 5/5 kryteriow zielonych i utknelo na `harness_workflow_two_kinds` — wyroczni
AC-2 z T-23, ktora asertuje **rownosc** zbioru rodzajow, nie zawieranie, z komentarzem
napisanym wprost: *„trzeci rodzaj po cichu dolozony, zeby graf sie zmiescil, jest dokladnie
ta awaria, ktora to zadanie ma lapac"*. Krok „sprawdz" JEST trzecim rodzajem. Wyrocznia
zadzialala dokladnie tak, jak zaprojektowano.

**Pisarz nie oslabil asercji** — zostawil plik nietkniety i pozwolil mu pasc, a piec innych
plikow dostalo po JEDNEJ linii ramienia `match`, ktorej wymaga kompilator. To jest zachowanie,
o ktore chodzi w AGENTS.md par. 7, i dlatego zostaje odnotowane.

Rozstrzygnal czlowiek: **zmieniamy D6** (`94a0d23`). Regula „nie powtarzamy funkcji vendorow"
zostaje w mocy bez jednej zmiany — zaden vendor nie dostarcza „uruchom komende i sam orzeknij,
czy przeszla". Zmienil sie tylko limit liczbowy, ktory tej reguly nie wyrazal.

**Czego to nie otwiera, zapisane w D6, zeby nie stalo sie precedensem:** nie ma i nie bedzie
kafelka „recenzja" — etap nazwany w kodzie JEST domyslny i nie da sie go wylaczyc konfiguracja
(D7, niezmiennik 27). Wyrocznia T-23 dostala wlasnie ten rodzaj jako swoj nowy przypadek
odmowy, wiec regula jest **egzekwowana mechanicznie**, a nie tylko napisana.

### Jedna stala odpowiadala na dwa pytania

Przy okazji wyszlo, ze `KNOWN` w tej wyroczni znaczylo naraz „co zna schemat" i „czego uzywa
mierzony plik" — i moglo, dopoki odpowiedz byla ta sama. Po dolozeniu `check` przestala:
schemat zna trzy, a `ship-task.json` uzywa dwoch, bo etapy sprawdzenia i wejscia na trunk stoja
w nim NADAL na kafelku kontrolnym. Stala nazywa sie teraz `IN_THE_FILE` i pilnuje pliku, bo
asercja od poczatku byla o pliku. **Przepisanie `s_gate` i `s_land` na kroki sprawdzenia jest
osobna praca** i tak stoi w komentarzu.

### T-57: dlug po T-54 splacony, nie zamieciony

T-54 wyladowalo z czterema funkcjami bez konsumenta produkcyjnego (`plugin_dir`, `plugin_argv`,
`recurring_patterns`, `agent_body`) — wolanymi wylacznie z `tests/`, czyli z osobnych skrzyn,
w ktorych `dead_code` milczy. `quick-wired` zlapal to i zaoferowal dwa wyjscia; wybrane zostalo
drugie, ktore sam check opisuje jako „przeniesienie dlugu tam, gdzie ktos go widzi": napisane
**T-57** z czterema prawdziwymi kryteriami, ktore te funkcje wolaja. Wyladowalo tej samej nocy.

### Falszywa czerwien, ktora kosztowala jedno przejscie

T-57 zglosilo `full-test` czerwone z „vitest exited 0 and reports no passing tests / no Tests
line at all", przy 4/4 kryteriach zielonych. To bylo obciazenie maszyny (rownolegly bieg T-55),
nie defekt: ta sama galaz na spokojnej maszynie daje **152 pliki / 817 testow**. Rozpoznanie
jest jednolinijkowe — odpal `npx --no-install vitest run` na galezi i na trunku i porownaj.

### Dwa biegi zginely na granicy tury — i to jest naprawione

T-10 i T-54 zostaly ubite na twardym suficie 3600 s tla, oba w fazie recenzji albo poprawek,
czyli PO wykonaniu pracy. Zero osieroconych procesow (sprawdzone `ps` po `claude -p`).
Rozwiazanie: `scratchpad/detach.py` — podwojny fork + `setsid`, kod wyjscia do `<log>.rc`.
T-55 i T-57 poszly odczepione i przezyly. **Helper nie jest w repo** i przy nastepnej sesji
trzeba go napisac od nowa albo wpiac na stale.

## 2026-08-19, 22:20 — harness jest NASZ: dziedziczymy tekst, nigdy maszynerie

**Wyladowane: T-53 (4 kryteria) i T-10 (6).** Pelna bramka po kazdym: 15 sprawdzen, 0 czerwonych.
Do tego zamkniety spike **S-3** i **trzy naprawy harnessu**, kazda z kontrola w obie strony.

Pytanie wlasciciela brzmialo: co sie stanie, gdy Loadout odpali agentow w repo, ktore ma juz
WLASNY harness (mierzone na `../meetnotes`, ale to tylko przyklad). Odpowiedz jest zmierzona,
nie zalozona, i odwrotna do pierwszej hipotezy.

### Kierunek „wczytaj ustawienia gospodarza, odejmij haki" NIE ISTNIEJE

Zmierzone na 11 biegach `claude -p`: kazdy z `--setting-sources project` odpalil hak gospodarza
(7/7); `--settings <plik>` SUMUJE sie z projektowym i nie gasi hakow nawet podana pusta lista
`PreToolUse`; `--bare` gasi je kosztem OAuth (`Not logged in`), wiec na subskrypcji jest
bezuzyteczny. Zostaje kierunek odwrotny: **odetnij wszystko, potem odbuduj wiedze po swojemu.**

Cena wczytania jest twarda, nie estetyczna: **hak PreToolUse gospodarza startuje proces we
WLASNEJ grupie procesow, a jego dziecko dostaje ppid=1 i przezywa wyjscie `claude`.** Zmierzone:
jeden bieg zostawil 14 sierot, eksperymenty lacznie 30 zywych procesow ubitych recznie. Przy
zaladowanych ustawieniach gospodarza **niezmiennik 6 jest nie do utrzymania** — zabicie naszej
grupy nie dotyka ani jednej z tamtych.

Zmierzone ryzyko, ktore ta fala zamyka: nasz agent wywolal projektowego podagenta gospodarza
(`release-engineer`), ktory wystartowal jako osobny proces i spalil **38-41 tys. tokenow
calkowicie poza widokiem i rozliczeniem Loadouta**.

### Dwie rzeczy, w ktorych mylil sie research po drodze

1. **`--allowedTools` to lista AUTO-ZATWIERDZANIA, nie filtr dostepnosci.** `Task`/`Agent`
   i `Skill` sa dostepne w KAZDEJ z trzech polityk. Filtrem jest `--tools` — twarda biala lista, i to
   ona wchodzi do sterownika (T-53 AC-1). Czarna lista nie wystarcza: domyslna powierzchnia ma
   osiem sciezek startu procesu (Task, Workflow, SendMessage, CronCreate, RemoteTrigger,
   ScheduleWakeup, EnterWorktree, Monitor) i cicho urosnie przy nastepnym wydaniu CLI.
2. **`init.tools` nie jest powierzchnia uprawnien.** Lista pod `ReadOnly` zawiera `Bash`.
   Porownywanie polityk przez dlugosc tej listy to blad kategorii — 27 pozycji to BAZA CLI,
   a wymienienie `Glob` albo `Grep` w `--allowedTools` odslania oba, dajac 29.

### Ustawienia gospodarza moga nas ROZSZERZYC, nie tylko zawezic

`sandbox.autoAllowBashIfSandboxed: true` przepuszcza dowolna komende mimo naszego
`--allowedTools`. Blok `env` gospodarza nadpisuje srodowisko podane przez Loadouta (jego haki
czytaja wlasne zmienne, wiec haki i `env` to jedna calosc). Dlatego przepisujemy **wylacznie
`permissions.deny`** — `src-tauri/src/engine/drivers/host.rs`, T-53 AC-4.

### Trzy naprawy harnessu, kazda po prawdziwym incydencie

- **`ac30479` — cztery konsumenty OWNS czytaly ten blok na trzy rozne sposoby.** 42 z 60 plikow
  zadan konczy blok bajtami `...cancel.rs-->`, bez nowej linii. `quick-scope.sh` kasowal `sed '$d'`
  CALA ostatnia linie (ginela ostatnia sciezka), a `before-spec-owns.sh` z regexem `\n-->`
  **nie dopasowywal wcale** i wychodzil zerem z napisem „nothing to judge" — czyli NIE SADZIL
  NICZEGO na 42 zadaniach. To niezmiennik 19 zlamany po cichu wewnatrz bramki. T-10 wpadl przez
  to w pelne zakleszczenie: napiszesz plik -> `quick-scope` czerwony, nie napiszesz -> AC-6
  czerwone, TASK.md zablokowany.
- **`04a346e` — kanarek `tasks/T-01.md` pilnowal polityki, ktora wlasciciel cofnal** commitem
  `533eab8`. T-53 skonczylo 4/4 zielone i utknelo na czerwieni, ktorej zadna dozwolona sciezka
  nie gasi. Zdjecie jest bezpieczne: `Edit/Write(TASK.md)` zostaja w `deny`, wiec pisarz dalej
  nie tknie wlasnego kontraktu.
- **`699ef25` — kod 2 znaczy „nie twoje" na calej dlugosci.** `quick-permissions` oddawal 1 przy
  sprzecznosci konfiguracji, choc CALY jego material (`.claude/settings.json`, blok OWNS, on sam)
  lezy poza zasiegiem pisarza. Teraz oddaje 2. Razem z tym **zawezona karta w `integrate.sh`**:
  stara wersja wybaczala KAZDY kod 2 na trunku, wiec sama pierwsza naprawa otworzylaby dziure.
  Wybacza teraz wylacznie przy SWIEZYM paragonie z pusta lista `misconfigured` (nowe pole w
  `runs/last.json`); brak paragonu i paragon o innym commicie znacza odmowe.

**Zasada dla nastepnych sprawdzen:** sprawdzenie, ktorego caly material lezy poza zasiegiem
pisarza, oddaje 2, nie 1.

### S-3 zamkniety, T-10 odblokowane — ale pokrycie parsera jest zdegradowane

`docs/research/fixtures/codex-stream.jsonl` pochodzi z PRAWDZIWEGO biegu `codex exec --json`
(commit `7a24fd4`), ale zawiera wylacznie **koperte awaryjna**: cztery zdarzenia
(`thread.started`, `turn.started`, `error`, `turn.failed`), bo konto Codeksa jest bez kredytow
**do 2026-08-20 05:30**. Ani jednego `item.*`. T-10 AC-2 przewidzialo ten przypadek i wymaga
oznaczenia mapowan `item.*` komentarzem `[3p]`. **Po 5:30 S-3 leci ponownie i ten plik ma sie
POWIEKSZYC** — to jest zaplanowane, nie regresja.

Dwa pomiary przy okazji: stdout Codeksa jest czystym JSONL, a stderr niesie `Reading additional
input from stdin...` (potwierdza T2 §9.3: nigdy `2>&1`). `--ignore-user-config` USUWA ladowanie
cudzych serwerow MCP — bieg bez tej flagi probowal odswiezyc OAuth dla figma, notion i linear,
zanim ruszyla tura. Codex nie ma `--strict-mcp-config`, wiec to jedyny znany srodek.

### Codex jest slabszym adapterem i to trzeba zapisac, a nie zalozyc symetrie

Nie ma odpowiednika `--tools`, `--disallowedTools` ani `--setting-sources`. `--ignore-user-config`
tyka WYLACZNIE `$CODEX_HOME/config.toml`, a `--ignore-rules` tylko pliki `.rules` — **zadna flaga
nie wylacza projektowego `.codex/hooks.json` gospodarza** (meetnotes ma tam te same trzy straze
co po stronie Claude'a). Jedyna obrona to zaufanie hakow po haszu tresci, czyli obrona MASZYNY,
nie Loadouta: hak raz zatwierdzony wystartuje. Dla adaptera: piaskownica (`-s read-only` /
`workspace-write`) jest glowna dzwignia, `--ephemeral` bez zapisu sesji, i **nigdy**
`--dangerously-bypass-hook-trust`.

### Co czeka

| co | stan |
|---|---|
| **T-54** — dziedziczenie wiedzy (5 kryteriow) | **w biegu**, faza kontraktu |
| **T-55** — krok „sprawdz" (5 kryteriow) | napisane, czeka na wolna maszyne |
| **T-56** — jedna kopia dla lancucha + krok ciezki (2) | napisane, **czeka na T-52** |
| **T-52** — izolacja jako drzewo gita | napisane przez wlasciciela, galaz `T-52`, niezlandowane |
| S-3 ponownie + przeglad cross-vendor | po 2026-08-20 05:30 |

**Wada, ktorej ta fala NIE zamyka:** bramka dalej nie odroznia „czerwien z mojego zakresu" od
„czerwien odziedziczona z trunku w trakcie biegu". `refresh_harness_from_trunk` jest projektowane
i moze wniesc czerwien, ktorej zadanie nie spowodowalo — T-53 musialo zglosic defekt konfiguracji
(semantyka kodu 2) pod kodem 1, bo nie ma czym powiedziec tego inaczej. `699ef25` zamyka tylko te
klase, w ktorej sprawdzenie SAMO wie, ze sadzi nasza konfiguracje.

## 2026-08-19 — sekcja Skills umie przyjac tresc, nie tylko adres

**Wyladowane: T-42 (4 kryteria) i T-43 (3).** Pelna bramka po kazdym: 15 sprawdzen, 0 czerwonych.
Zamowienie czlowieka brzmialo „chce napisac jakiego chce skilla, a program buduje z niego skilla
kompatybilnego z claude/codex", z wyborem „opis -> agent pisze". Rozbite na trzy kontrakty, bo to
sa trzy rozne dowody: **T-42** droga wejscia dla TRESCI (trzy pytania -> `place::emit` -> zapis ->
`ingest::from_folder`, ten sam skan co przy linku), **T-43** jedna tura agenta POZA grafem
(`AgentDriver::start` -> `Outcome.text` -> trzy pola formularza), **T-44** wybor „ten projekt /
wszedzie" (w toku).

### Co z tego wynika dla produktu

Zlota lista komend: 24 -> 29 (`author_skill`, `draft_skill`, `stop_draft` z tej fali, `open_chat`
i `say_to_orchestrator` z pracy wlasciciela). Karta przegladu przestala twierdzic, ze wie, skad
przyszla umiejetnosc: plakietka „From the internet" byla wpisana NA SZTYWNO i ignorowala
`item.fromTheInternet` -- prawdziwa przez konstrukcje, dopoki jedyna droga byl link. Pochodzenie
lezy teraz w plikach (`~/.loadout/skills/origins.json`), a nie w domysle z istnienia kopii
kanonicznej, i ma ostrozny domyslny: kopia bez zapisu pochodzenia jest „z internetu", bo do tej
fali tylko taka droga tworzyla kopie.

### Trzy znaleziska, ktorych ta fala NIE zamiata (AGENTS.md §7)

1. **Utrata danych osiagalna z okna, naprawiona po drodze w T-42 AC-1(c).** `review_skill_inner`
   liczyl sciezke kopii kanonicznej z pola `name` front-mattera i robil na niej `remove_dir_all`
   (`commands/skills.rs:350-351`); `from_folder` nie waliduje nazwy, a `Skill::default()` daje
   `name: ""`. Sprawdzone `rustc`: `PathBuf::from("/a/b").join("")` to `"/a/b/"`. Link do dowolnego
   `SKILL.md` BEZ pola `name:` kasowal `~/.loadout/skills/` razem z `installed.json`.
2. **Globalny limit „ile naraz" nie jest podpiety w produkcji.** `run_workflow_with_slots(…, slots)`
   nie ma wolajacego poza testami, a `run_workflow_inner:237` zaklada wlasny `Limiter` na kazdy
   bieg. Kryterium T-31 dowodzi globalnosci, bo podaje pule argumentem. Trzy karty po trzech
   agentach to dziewieciu agentow przy suwaku na 3 (niezmiennik 11). Dlatego T-43 nie udaje, ze
   bierze slot -- ma jawna granice „jeden draft naraz".
3. **Lista pol zdjetych przez `emit` nie ma konsumenta** (`let (doc, _) = emit(skill)`,
   `place.rs:545`). `hooks:` znika z pliku bez ani jednego zdania na ekranie. Do tego
   `allowed-tools` jest w `SPEC_FIELDS`, wiec JEDZIE do obu katalogow vendorow z samym `Warn` --
   umiejetnosc moze przydzielic sobie narzedzia, a przy tekscie pisanym przez model przestaje to
   byc rzadkie.

### Dwa defekty harnessu, naprawione osobnymi commitami

Odslonil je stos galezi (T-43 odbity od niewyladowanego `task-T-42`, bo trunk byl brudny). Oba
mialy ten sam ksztalt: pytanie o stan dysku rozstrzygane po BRZMIENIU komunikatu.

- `0140979` -- `exit 0 but no evidence` bylo liczone jako „kryterium przechodzi", wiec kazdy
  wznowiony bieg z kryterium rustowym konczyl sie kodem 2 przy uczciwie czerwonych kryteriach.
- `c696fc0` -- „czy sa specyfikacje" rozstrzygane po napisie `did not RUN`; kryterium rustowe bez
  modulu udawalo istniejacy plik, wiec bieg szedl NAPRAWIAC pliki, ktorych nie ma. Teraz pyta
  dysku przez `gate.spec_tokens` -- ten sam parser, ktory sadzi kontrakt.

Oba z kontrola w obie strony na prawdziwych bajtach funkcji; grozny przypadek („PASSES before
implementation") dalej odmawia.

### Cena infrastruktury, zmierzona

T-42 kosztowalo **~$36,50**, z czego **$12,15 to strata na infrastrukturze**: limit sesji (429 po
811 ms, faza pisarza nie ruszyla) i ubicie biegu na granicy tury (7 minut pisania, `result:
error_during_execution`, $8,44 za prace, ktorej nikt nie odebral). Zamkniete przez
`scratchpad/detach.py` (podwojny fork + `setsid`, kod wyjscia do `runs/<ID>/wave.rc`) -- ten sam
bieg odczepiony przezyl cztery granice tury. Do czekania na wynik uzywaj `Monitor` z
`persistent: true`, nie `run_in_background`: czekacz ginie na kazdej granicy tury, praca nie.

**Falszywa czerwien, ktorej nie warto szukac drugi raz:** `product_path_end_to_end`,
`run_reaches_the_pump`, `runcmd_snapshot` i `runcmd_parallel` wieszaja sie na ZAJETEJ maszynie --
mierza nakladanie sie na prawdziwym zegarze i maja limit 20 s w sobie, wiec
`CHECK_TIMEOUT_OVERRIDE` ich nie podniesie. Przy siedmiu agentach w tle: cztery czerwone.
Na bezczynnej maszynie ta sama migawka: 15 sprawdzen, 0 czerwonych, 16 s.

## 2026-08-18, 05:30 — pietnascie kryteriow jednego dnia i aplikacja, ktora naprawde chodzi

**Suita jednostkowa: 88 plikow / 440 testow zielonych. E2E w prawdziwym chromium: 13/13.**
Dowiezione tego dnia: T-37 (3 kryteria), T-38 (8), T-39 (7). Kontroli negatywnych: **101 w piecu
rownoleglych pasach plus 3 moje**, wszystkie czerwone, wszystkie przywrocone po md5.

### Aplikacja dziala — zmierzone, nie zadeklarowane

Zrzut zywego okna 05:24 pokazuje menu 196 px ze znakiem i `LOADOUT`, piec sekcji, stopke
`Claude · Codex ready`, pasek kart z `＋`, wybor workflow, **wlaczony** `Start`, suwak „ile
naraz", pusty stan z zaproszeniem, **szyne agentow** po prawej i **wiersz wejscia** na dole.

**Dowod, ze to nie atrapa:** w wyborze stoi `New workflow 2`, a na dysku lezy
`~/.loadout/workflows/new-workflow-2.json` z polem `"name": "New workflow 2"`. Lancuch
plik → `list_workflows` → `invoke` → okno jest prawdziwy w obie strony — te pliki powstaly
wczesniej przyciskiem `Create`.

### Biale okno przy starcie — przyczyna zamknieta, NIE jest defektem produktu

Dwie przyczyny, obie srodowiskowe. (1) `tauri dev` obserwuje `src-tauri/` i **restartuje
aplikacje po kazdym zapisie** — przy pieciu agentach piszacych rownolegle okno ginelo co
kilkadziesiat sekund, a czlowiek widzial „szary ekran na chwile". (2) vite pre-bunduje
zaleznosci na zadanie i pierwsze wejscie po zmianie ich zestawu blokuje `/src/main.tsx`
na **32 s**; webview trzyma wtedy polaczenia i pokazuje pusta strone.
**Rozpoznanie jest jednolinijkowe:** `curl -o /dev/null -w '%{time_total}' /src/main.tsx`
mierzy ten czas wprost. Okno maluje sie natychmiast po tym, jak serwer zaczyna oddawac modul.

### Trzy rzeczy, ktore znalazl dopiero sprawdzajacy

Pieciu niezaleznych sprawdzajacych z poleceniem „domyslaj sie na niekorzysc pasa". Kazdy
odtworzyl po jednej kontroli negatywnej SAM i przeszukal pliki pod katem zaslepek.
**Atrap nie znalezli zadnych.** Znalezli trzy rzeczy, ktorych nie widzialo zadne kryterium:

1. **Zamkniecie karty z zywym biegiem nie anulowalo biegu.** `WorkspaceTab.agents` bylo pisane
   tylko przy zakladaniu karty i zawsze zerem, wiec `requestClose` zawsze wchodzil w galaz
   „nic tu nie chodzi": karta znikala bez pytania i bez `cancel()`. Osierocony agent dalej palil
   limit (niezmiennik 6 — blad finansowy). `CloseConfirm` byl przez to kodem NIEOSIAGALNYM.
   Naprawione, kryterium T-39 AC-7 z trzema sondami.
2. **`useMemory.load` i `useSkills.load` nie mialy wolajacego** — sciezka odczytu byla zbudowana
   i martwa, wiec obie sekcje dalej nie czytaly dysku. Naprawione.
3. **`commands-wired.test.ts` byl czerwony**: doszly dwie krawedzie bez wiersza w tabeli strazy.
   Dopisane, 16 → 18.

### Co zostalo do prod-ready

- **T-41 (napisane)** — odpowiedz czlowieka NIE dochodzi do agenta. `answer()` jest czysto
  lokalne: pytanie znika z ekranu, agent dalej czeka. To jedyna znana martwa kontrolka i jedyna,
  ktora **klamie**. Nie jest to podpiecie kabla — `RunControl` nie ma uchwytu do zywej sesji,
  wiec trzeba przeciagnac kanal przez granice. `AgentDriver::send` juz istnieje.
- **T-40 (napisane)** — wyrocznia „kazda kontrolka cos robi" poza pieciu ekranami: stany
  zagniezdzone, pola i selecty, oraz dowod, ze skutek jest TYM skutkiem.
- **`quick-types` nie umie byc czerwony na kodzie zadania** — prawdziwy blad typow melduje jako
  „our TypeScript configuration is broken — this is not your code", kodem 2, o ktorym bramka
  sama pisze „never a red". Trafilo mnie dwa razy jednego dnia.
- **`tests/it/main.rs` to nowy kregoslup bez `merge=union` i bez wlasciciela** — dwa zadania
  dodajace test naraz dadza pewny konflikt.

## Liczby

| | |
|---|---|
| commitów lądowania | **34** |
| trunk | **ZIELONY** — 14 sprawdzeń, 0 porażek, 390 s (`runs/last.json`, 2026-08-18 00:31) |
| zadań w `tasks/` | 42 |
| żywe gałęzie | **cztery**: T-38 · T-29 · T-28 · T-37 (T-32 wylądowało, worktree do sprzątnięcia) |
| zablokowane kalendarzem | **S-3, T-10** — kredyty Codeksa wracają 2026-08-20 |

## Co się dziś zmieniło w tempie pracy — i dlaczego to jest najważniejszy wpis

| | było | jest |
|---|---|---|
| `checks/full-test.sh` | do 3600 s, czyli **timeout** | **224 s** |
| `cargo clippy --all-targets` | 455–1200 s | **6 s** na ciepłym drzewie |
| `./verify.sh quick` | ~300 s | **37 s** |
| `./verify.sh full` na trunku | nie kończyło się | **390 s** |
| lądowanie gałęzi | ~2 h | **9–11 min** |

**Przyczyna była jedna: `src-tauri/tests/` miało 122 pliki, a Rust robi z każdego pliku osobne
binarium** linkujące całą bibliotekę z 527 skrzyniami. Same testy wykonują się w **6,0 s**;
reszta była składaniem i pierwszym uruchamianiem 122 programów. Dwie niezależne miary tego samego:

- linkowanie — kontrolowany pomiar jednego celu po dotknięciu `commands/run.rs`: **60 s i 62 s**;
- **pierwsze** uruchomienie świeżej, niepodpisanej binarki debug — `store_strict_schema` **36 s**,
  `workflow_check_ids` **59 s**, przy **0 s** za drugim razem i teście trwającym 0,01 s. To jest
  skanowanie macOS (`syspolicyd`, `XprotectService`), zapamiętywane per plik.

Obie miary mnożyły się przez 122. Pliki są teraz **modułami jednego celu** (`tests/it/main.rs`),
czyli jeden link i jedno skanowanie. Tak samo robią ripgrep (`autotests = false` + jeden
`[[test]]`) i cargo (`tests/testsuite/main.rs`, ~150 linii `mod`). `src-tauri/Cargo.toml` sam to
deklarował od pierwszego dnia — „`cargo test --lib` jest CAŁĄ powierzchnią testową" — a kod łamał
tę deklarację 122 razy.

Dla skali, zmierzone na tej maszynie: `../meetnotes` ma **950** skrzyń (prawie dwa razy więcej
niż my) i **jedno** binarium testowe — 19 835 plików w `target/debug/deps` wobec naszych 886 645.

### Trzy rzeczy, które z tego wynikają dla piszącego

1. **Kryterium woła `cargo test --test it <moduł>::`**, nie `cargo test --test <moduł>`. Filtr
   z dwukropkami, nie sam podciąg: `--test it store` łapie także `store_pragmas` i `storage_x`.
2. **Nowy plik w `tests/it/` wymaga linii `mod` w `main.rs`.** Bez niej nie kompiluje się, nie
   uruchamia ani jednego testu i **wygląda jak zestaw, który przeszedł**. Pilnuje tego
   `checks/quick-tests-listed.sh` — mechaniczny, bez kompilacji, więc działa też wtedy, gdy
   drzewo się nie buduje.
3. **Test mierzący albo zmieniający stan CAŁEGO PROCESU zostaje osobnym celem w `tests/`.**
   Dziś dwa: `shell_logging` (liczy deskryptory przez `/dev/fd`, instaluje globalny hak paniki)
   i `supervisor_env_hygiene` (`env::set_var`). W scalonym binarium mierzyłyby 285 cudzych
   testów — `shell_logging` dostał 96 zamiast swojej liczby przy pierwszym lądowaniu po scaleniu.

## Praca, która weszła z pominięciem pętli — i co to kosztuje

Cztery zadania weszły **wprost na trunk**, bez gałęzi i bez tieru `before`: **T-28, T-33, T-35,
T-37**. Powód był policzalny — fala kosztowała ~2 h przy około 40% skuteczności za pierwszym
razem — ale konsekwencja jest realna i zostaje zapisana:

**Te cztery nie mają dowodu, że ich kryteria były najpierw CZERWONE**, ani drugiej opinii.
Kryterium za wąskie od urodzenia jest w nich niewykryte.

**Powód tego skrótu zniknął.** Pełna bramka idzie 9 minut. Cztery przebiegi to niecała godzina
i to jest najtańszy sposób odzyskania tego dowodu.

Najlepszy argument za tym stoi w `f35466f`: pełna bramka na trunku złapała **prawdziwe
naruszenie projektu** we wczorajszej pracy — `libc::getpgrp()` w `lib.rs` zamiast w
`supervisor.rs` (niezmiennik 3, dwa sprawdzenia naraz). Bez niej nikt by tego nie zobaczył.

## Co stoi w trunku, a czego nie widać z plików zadań

- **Aplikacja się uruchamia i zapisuje.** Cztery wady widoczne dopiero z prawdziwego okna:
  białe okno od IPv6 (`host: false` wiązało serwer na `::1`, WKWebView pyta o IPv4 i **nie
  zgłasza żadnego błędu**), brak `.manage()` (trzy komendy biegu padały „state not managed"),
  katalog projektu wskazujący na `src-tauri/`, oraz `Store::open` poza runtime'em tokio.
- **Sekcje są podpięte do prawdziwych adapterów.** Do 2026-08-17 wszystkie pięć `io.ts` istniało
  i **żadnego nie wołał kod produkcyjny** — jedynym importerem był test. Ekrany były trwale puste,
  a Create odmawiał pod palcem.
- **Edytor workflow jest osiągalny.** Płótno i panel kroku miały testy i **ani jednego miejsca
  montowania**. Siedem takich komponentów znalazł jeden pomiar; `checks/quick-wired.sh` pilnuje
  teraz strony Rusta, strona TS została jako dług.
- **„Własna kopia twoich plików" znaczy kopię** (T-33). Wcześniej `fresh-copy` dawał pusty
  katalog, więc krok pracował na pustce — gorzej niż kolizja, bo agent nie widzi plików.
- **Krok ma limit czasu** (T-35 AC-1), egzekwowany **przez sterownik**, nie przez
  `tokio::time::timeout` — tamto anuluje zadanie Rusta i zostawia żywy proces (niezmiennik 10).
- **Odzyskiwanie po awarii biegnie przy starcie okna** (T-35 AC-2/AC-3). Wymagało zbudowania
  **sześciu** brakujących ogniw, z których **pięć było w komentarzach opisanych jako istniejące**:
  odczyt `kern.boottime`, kolumna `boot_id`, `add_column_if_missing`, `reap_group`
  (`unimplemented!()`), odczyt wierszy i zapis znacznika przy starcie biegu.

## Wada, która wraca — nazwana, bo trafiona ponad dziesięć razy

**Kryterium sprawdza coś węższego niż niezmiennik, którego pilnuje.** Wzorcowy przykład: asercja
`TITLEBAR_HEIGHT <= 96` była **zielona przy 138 px** realnego chrome, bo mierzyła jeden pasek
z trzech. Drugi: „strzałka znaczy po" porównywała chwile odbioru paczek, więc padała losowo,
gdy dwa kroki trafiły w to samo 16-milisekundowe okno pompy.

Trzy rzeczy, które to rozróżniają, i wszystkie trzy są w tym repo sprawdzone:

1. **Wartość oczekiwaną czytaj z pliku, nie przepisuj.** Test wpisujący `196` z palca przechodzi
   też wtedy, gdy makieta mówi 220.
2. **Kontrola negatywna do każdego kryterium.** Dwie moje dzisiejsze sondy przechodziły **także
   przed poprawką** — dowiedziałem się tego wyłącznie dlatego, że je zasadziłem.
3. **Napisz w nagłówku, jaka byłaby SŁABA wersja tej asercji i co ją odróżnia.**

## Co dalej, po właścicielu

| co | kto | stan |
|---|---|---|
| T-38 — szew front↔Rust, klucze argumentów | agent redesignu | 8 kryteriów, gałąź `T-38` |
| T-37 — trzy testy kryteriów układu | agent redesignu | **kod w trunku, testów nie ma** |
| T-29 — e2e w przeglądarce | agent redesignu | odłożone świadomie do po redesignie |
| S-3, T-10 — drugi vendor | czeka | kredyty Codeksa 2026-08-20; `drivers/absent.rs` odmawia głośno zamiast udawać Claude'a |
| przepuścić T-28/T-33/T-35/T-37 przez pętlę | orchestrator | ~1 h, odzyskuje dowód czerwieni |
| Q-6 — zegar ścienny nie odróżnia „wolne" od „wisi" | kolejka | `docs/HARNESS-QUEUE.md` |
| Q-7 — liczba celów testowych | **zamknięte** | 122 → 1, opisane wyżej |

## Mina, o której trzeba pamiętać przed lądowaniem T-28

`a7a2d87` dodał oba pliki testów szkieletowych **wprost na main**, a gałąź `task-T-28` niesie
**własną, rozjechaną kopię tych samych plików**. Różnicą jest dokładnie `#[ignore]` z `6e55daf`,
czyli ogrodzenie płatnych testów uruchamiających prawdziwe procesy `claude`.

**Lądowanie T-28 bez uzgodnienia po cichu cofnie to ogrodzenie** — a bramka po takim lądowaniu
będzie **zielona**, bo cofnięte kryterium nie psuje testów, tylko je osłabia.
