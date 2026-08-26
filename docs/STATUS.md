# Stan budowy — 2026-08-18, 00:40

Ten plik jest **żywy**. Aktualizuje go orchestrator po każdym lądowaniu. Prawdą o zadaniu jest
`tasks/<ID>.md`; tutaj jest wyłącznie to, czego z plików zadań nie widać: co już stoi w trunku,
co stanęło i dlaczego.

## 2026-08-26, 16:56 — T-138 ZAMKNIĘTE: 18/19 po naprawie i niezałatana luka tombstone'a

**T-138 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 44 min 11 s Harnessu · $0,00
widoczne.** Dwie zapisane tury Codeksa zużyły 26,06 mln tokenów wejścia (25,54 mln z cache)
i 80,1 tys. wyjścia; recenzja, plan i wykonanie naprawy nie mają wspólnego paragonu tokenów.
Enforced `before` certyfikowało wszystkie trzy nowe targety w 7,06 s.

Implementacja przejęła pięć dozwolonych commitów T-137 i domknęła pełny workflow dwóch
korzeni, działającą refleksję z wrapperami i evidence, bajtowy snapshot, trwały Move oraz
adresowane akcje okna. Pierwsza pełna bramka miała **18/19** w 54,71 s: pełna suita i AC-1,
AC-2, AC-3 przeszły, a `full-clippy` znalazł identyczne ramiona `match` w nowym oracle Move.

Recenzent zgłosił jedną zasadną lukę medium. Oracle tombstone'a wołał drogę jednego korzenia,
więc nie dowodził, że dokładny tombstone w bibliotece blokuje automatyczny zapis do korzenia
projektu. Plan naprawy wskazał właściwą scenę przez `record_project_candidate_from_run`, lecz
wykonawca uznał zmianę oracle za wymagającą decyzji człowieka i jej nie wykonał. Połączył tylko
ramiona `match` w `b997767`; kolejna pełna bramka ujawniła drugi lint w tym samym nowym
targetcie: funkcja snapshot/reflection ma 131 linii przy limicie 100.

Końcowa bramka na czystym `b997767` ponownie miała **18/19** w 45,81 s. Wszystkie testy i
trzy AC są zielone; jedyną formalną czerwienią jest `full-clippy`, ale luka bibliotecznego
tombstone'a pozostaje prawdziwa niezależnie od wyniku bramki. Po jednej rundzie nie ma piątej
tury, ręcznego refaktoru ani lądowania.

Gałąź `task-T-138`, czysty worktree i `runs/T-138/` pozostają dowodem. Świeży następca musi
mieć trzy nowe globalnie unikalne targety, od początku rozbić długą funkcję oracle i dodać
wielokorzeniową scenę tombstone'a. Dopiero po własnym czerwonym `before` może jawnie przejąć
produkcyjne commity T-138: `705f433`, `7d9bbc9`, `124cc46`, `6642567`, `5ceea68`, `d439a25`
i `3dba18d`. Nie przenosi `f782ef9`, `330a49d`, `9e8ec91`, `b997767`, targetów T-138 ani
całej gałęzi.

## 2026-08-26, 14:18 — T-138 przejmuje H16/H18 bez powtarzania implementacji

Właściciel polecił przyspieszyć po obowiązkowym postoju T-137. Świeże **T-138** startuje z
czystego `main` i ma trzy nowe standalone targety. Dopiero po uczciwym `before` wolno mu
zastosować pięć commitów implementacyjnych T-137; jego kontrakt, specy i cała gałąź nie są
wejściem.

AC-1 prowadzi atrapę refleksji przez prawdziwe wrappery ustawień, evidence i budżetu, wymaga
receipt oraz fizycznej notatki tylko we właściwym projekcie, a stempel porównuje na
niekanonicznym front matterze bajt po bajcie po normalizacji wyłącznie `last_used_at`. AC-2
uznaje wpis Move dopiero po udanym delegowaniu rzeczywistej operacji i wymaga, żeby odmowa nie
udawała wykonanego fsync/publish/unlink. AC-3 przypina legacy do widocznej strefy
`earlier-project`, wyklucza je z `suggested` i zachowuje prawdziwe kliknięcia Move, Use, Stop
oraz Discard. Operacyjnie bieg pozostaje Codex + Codex; T-129/T-130 czekają na wylądowanie
pełnego adresu z T-138.

## 2026-08-26, 14:05 — T-137 ZAMKNIĘTE: AC-1 zatrzymała wadliwa atrapa refleksji

**T-137 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 55 min 36 s Harnessu · $0,00
widoczne.** Dwie zapisane tury Codeksa zużyły 27,12 mln tokenów wejścia (26,50 mln z cache)
i 92,2 tys. wyjścia; recenzja, plan i wykonanie naprawy nie mają wspólnego paragonu tokenów.
Enforced `before` certyfikowało trzy nowe standalone targety. Pierwsza bramka miała 17/19:
AC-2 i AC-3 przeszły, a `full-test` oraz AC-1 zatrzymały się na tym samym teście snapshotu.

Jedyna naprawa poprawiła prawdziwy defekt produkcji w `8345b02`: stempel `last_used_at`
zastępuje teraz wyłącznie tę linię w bajtach zamrożonego snapshotu, bez ponownego odczytu i bez
kanonizowania pozostałego front mattera. Końcowa bramka na czystym `8345b02` ponownie miała
**17/19** w 23,72 s. Pada jednak wcześniej niż dowód stempla: testowy `FakeDriver` zwraca
sterownik z `reflecting()`, ale nie implementuje wymaganych `with_settings`, `with_evidence`
i `with_budget`. Produkcyjny `reflection_driver` słusznie odmawia tak nieopakowanej atrapie,
więc notatka `T137-REFLECTION-A` nigdy nie powstaje i asercja katalogu projektu A jest czerwona.
To wada oracle do naprawienia w świeżym zadaniu, nie powód do zmiany polityki produktu.

Druga opinia pozostawiła jeszcze dwa zasadne braki dowodu. `RecordingMoveIo` zapisuje część
operacji przed delegowaniem, więc ślad może opisywać próbę zamiast wykonanego fsync/unlink.
Browserowy test znajduje legacy row globalnie, zamiast dowodzić, że stoi w strefie
`earlier-project` i nie stoi w `suggested`. Naprawa bajtów usuwa produkcyjną część trzeciej uwagi
o niekanonicznym front matterze, ale świeży oracle powinien użyć właśnie takiego fixture.

Gałąź `task-T-137`, czysty worktree na `8345b02` i `runs/T-137/` pozostają dowodem; nic nie
wylądowało. Po jednej rundzie nie ma piątej tury ani ręcznego łatania. Uczciwa kontynuacja to
świeży następca z globalnie unikalnymi targetami: po własnym czerwonym `before` może jawnie
przejąć wyłącznie commity implementacyjne T-137, lecz musi od zera postawić działający szew
refleksji, ślad Move rejestrowany dopiero po sukcesie oraz locator legacy przywiązany do
widocznej strefy.

## 2026-08-26, 12:52 — T-137 przejmuje H16/H18 z trzema obserwowalnymi wyroczniami

Właściciel polecił kontynuować po obowiązkowym postoju T-136. Świeże **T-137** startuje z
czystego `main` i ma trzy nowe standalone targety. Dopiero po uczciwym `before` wolno mu
wykorzystać trzy commity implementacyjne T-136; jego kontrakt, specy i jedyna naprawa nie są
wejściem.

AC-1 porównuje literalny multizbiór adresów, zmienia niesioną notatkę po przechwyceniu
`RunSpec.prompt`, lecz przed stemplem, i używa tombstone'a `similar-slug-extra__…` przeciw
`similar-slug`. AC-2 wykonuje ten sam rdzeń Move przez produkcyjny adapter oraz modelujące IO,
więc mierzy temp, zapis, oba fsync, no-clobber i unlink w kolejności zamiast ufać nazwom w
źródle. AC-3 zachowuje prawdziwe kliknięcia Move, Use, Stop i Discard oraz pełne katalogi po
każdej odpowiedzi. Operacyjnie bieg pozostaje Codex + Codex; T-129/T-130 czekają na
wylądowanie pełnego adresu z T-137.

## 2026-08-26, 11:44 — T-136 ZAMKNIĘTE: czerwona bramka i trzy luki wyroczni AC-1

**T-136 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 11 min 25 s Harnessu
(1 godz. 21 min 27 s od pierwszego startu wraz z odzyskiwaniem miejsca) · $0,00 widoczne.**
Dwie porównywalnie zapisane tury Codeksa zużyły co najmniej 27,36 mln tokenów wejścia
(26,85 mln z cache) i 80,3 tys. wyjścia; niedokończona naprawa kontraktu, recenzja, plan oraz
wykonanie naprawy nie mają wspólnego paragonu tokenów. Pierwszy start zatrzymał hostowy
`ENOSPC`; po usunięciu wyłącznie odbudowywalnych cache bieg wznowił ten sam certyfikowany
kontrakt i nie powtarzał `before`.

Implementacja w `8cca95a`, `eb09306` i `33d84f3` domknęła produkcyjny adres dwóch korzeni,
historyczne fixture oraz prawdziwe akcje okna. Pierwsza pełna bramka miała **15/18**:
AC-2 przeszło, AC-1 odrzuciło kolejność katalogu, a full Clippy znalazł podobne nazwy w nowym
teście. Druga opinia zgłosiła trzy zasadne luki AC-1: brak interleavingu zmieniającego notatkę
między przechwyceniem promptu a stemplem, brak obserwowalnego dowodu temp/no-clobber/fsync
w Move oraz tombstone, który nie rozróżnia dokładnego `<id>__` od dłuższego prefiksu.

Plan naprawy poprawnie rozpoznał, że kontrakt wymaga multizbioru, podczas gdy oracle
porównuje uporządkowany `Vec`; nie wolno naginać produkcyjnego sortowania do fixture.
Wykonawca zmienił jednak tylko pierwszą nazwę w `2c2b389`. Końcowa bramka na czystym drzewie
`f1b8d6b` ponownie miała **15/18** w 172,49 s: `full-clippy` znalazł nieużywane `&self`,
`full-test` oraz AC-1 nadal padały na katalogu, a samodzielny AC-1 raz ujawnił też niestabilny
scenariusz stempli. Po jednej rundzie nie ma piątej tury, ręcznego łatania ani lądowania.

Gałąź `task-T-136`, worktree i `runs/T-136/` pozostają dowodem. Świeży następca musi startować
z `main`, dostać nowe globalnie unikalne targety i dopiero po uczciwym `before` może jawnie
przejąć trzy commity implementacyjne T-136. Musi porównywać literalny multizbiór, usunąć lint
bez suppression oraz uczynić trzy uwagi recenzenta obserwowalnymi; samo zazielenienie obecnych
dwóch błędów byłoby słabsze od kontraktu i nie może być podstawą integracji.

## 2026-08-26, 10:18 — T-136 przejmuje pełny zakres zamkniętego T-128

Właściciel zatwierdził świeży kontrakt i przyszłe rutynowe decyzje wykonawcze fazy. **T-136**
startuje z czystego `main`, ma dwa nowe standalone targety oraz od początku posiada oba stare
pliki, które zamknęły T-128, i pozostałe fixture ujawnione przez pełną suitę. Po uczciwym
`before` wolno mu wykorzystać wyłącznie dwa produkcyjne commity T-128; stare commity
kontraktowe i testowe pozostają dowodem, nie wejściem.

Nowy Rust oracle rozróżnia stemple projektu B od stanu pozostawionego przez A i przypina
historyczne wyrocznie do właściwych fizycznych korzeni. Prawdziwy browserowy oracle klika
Move, Use, Stop oraz Discard i wymaga, żeby usunięcie projektowego duplikatu nie usunęło
bibliotecznej notatki o tym samym `id`. Operacyjnie bieg pozostaje Codex + Codex; następne
T-129/T-130 czekają na wylądowanie pełnego adresu z T-136.

## 2026-08-26, 09:28 — T-128 ZAMKNIĘTE: dwa stare testy są poza OWNS

**T-128 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 50 min 01 s Harnessu · $0,00
widoczne.** Enforced `before` certyfikowało oba standalone targety. Implementacja dwóch
korzeni, pełnego adresu `{ catalogFolder, place, id }`, Move, tombstone'ów i ochrony przed
spóźnioną odpowiedzią przeszła własne AC-1 i AC-2. Pierwsza pełna bramka miała 17/18:
789 testów projektu przeszło, pięć starych fixture nadal zakładało, że `this-project` leży
w bibliotece.

Recenzent znalazł jedną lukę medium i trzy niskie luki dowodu: E2E klikało Use, ale nie
Stop/Discard, projekt B nie dowodził własnych stempli, React key był sprawdzany po atrybucie,
a sekwencję trwałego Move dało się rozstrzygnąć dopiero inspekcją. Plan naprawy poprawnie
odmówił przywrócenia legacy do promptu, lecz wykonawca nie zmienił fixture i druga bramka
powtórzyła 17/18.

Po jawnej zgodzie właściciela test-only repair wzmocnił oba nowe oracle i przeniósł stare
fixture do korzenia projektu. `cargo check --all-targets --keep-going` przeszedł w 12,04 s,
ale pełna bramka mechanicznie odmówiła dwóm koniecznym plikom spoza `OWNS`:
`a_suggestion_can_be_discarded.rs` oraz `a_suggestion_needs_a_because.rs`; ujawniła też
kolejny historyczny adres w należącym do zadania `run_evidence_reaches_the_product.rs`.
To jest dokładny wynik z instrukcji fazy: nie rozszerzać OWNS, nie łatać kontraktu i zapisać
zadanie jako zamknięte. Gałąź `task-T-128` pozostaje niewylądowana; następca musi od początku
posiadać implementację dwóch korzeni oraz pełny zestaw starych wyroczni.

## 2026-08-26, 03:42 — T-104 ZAMKNIĘTE; T-128 przejmuje pełny adres dwóch korzeni

**T-104 · ZAMKNIĘTE / NIEURUCHOMIONE / NIEWYLĄDOWANE · $0,00.** Cztery rustowe `check:`
filtrują funkcje wspólnego targetu `tests/it`, więc nie mają globalnie unikalnej ścieżki i
mogą zazielenić pusty wybór. Kontrakt nie posiada też `AppState::project_for`, pełnego adresu
`{ place, id }`, konsumentów prawdziwego promptu ani regresji T-126. Uruchomienie go mimo
znanej wady byłoby spaleniem `before`, nie pomiarem zachowania.

Świeże **T-128** startuje z wylądowanego T-127 i ma dwa globalnie unikalne targety. Rozdziela
bibliotekę (`everywhere`, `this-agent`) od `<projekt>/.loadout/memory` (`this-project`),
adresuje każdą akcję przez zamrożone `{ catalogFolder, place, id }`, daje wcześniejszym
bibliotecznym notatkom projektowym trwały Move przed użyciem i nie pozwala spóźnionej
odpowiedzi poprzedniego workspace nadpisać ekranu. Ten sam runtime oracle dowodzi promptu,
stempla, dokładnego tombstone i braku wycieku między dwoma projektami. T-129 i T-130 przejmą
osobno bieżący stan budżetu/pochodzenia oraz zamrożony receipt rzeczywistych odbiorców.

## 2026-08-26, 03:17 — T-127 w trunku: prywatny stan każdego procesu Claude'a

**T-127 · zielone · 47 min 30 s do końca lądowania · $0,00 widoczne.** Pięć tur Codeksa
(kontrakt, implementacja, druga opinia, plan naprawy i wykonanie naprawy) zużyło łącznie
co najmniej 16,73 mln tokenów wejścia (16,10 mln z cache) i 66,4 tys. wyjścia. Uczciwe
`before` uruchomiło trzy samodzielne targety i padło na brakującym zachowaniu. Produkcja
izoluje każdą kopię pod `<run>/claude/<work-key>`, refleksję pod `_reflection`, nadpisuje
hostile `CLAUDE_CONFIG_DIR` po `env_clear` i odmawia widocznym zdaniem przed spawnem, gdy
prywatnego katalogu nie da się przygotować; stan gospodarza pozostaje nietknięty.

Pierwsza pełna bramka po implementacji miała zielone AC-1/2/3, ale globalny `full-test`
dwukrotnie trafił w hostowe `Operation not permitted` i brak boot-time. Druga opinia zgłosiła
dwie zasadne luki dowodu: brak późniejszej rundy tej samej kopii i brak jawnej ścieżki
`AgentEvent::Notice` → `Line::Problem`. Jedyna naprawa domknęła je w `285d112` i `c1b5ab7`;
dwa końcowe przebiegi gałęzi przeszły 19/19 w 43,00 s i 41,86 s. `integrate.sh` wylądował
wyłącznie `task-T-127` jako **`c5bcc5c`**; bramki `main` przed i po merge'u przeszły 16/16
w 115,57 s oraz 236,36 s. `TASK.md` nie przeżył lądowania, a trunk jest czysty.

## 2026-08-26, 02:24 — T-109 ZAMKNIĘTE; T-127 przejmuje izolację stanu przed spawnem

**T-109 · ZAMKNIĘTE / NIEURUCHOMIONE / NIEWYLĄDOWANE · $0,00.** Wszystkie trzy `check:`
filtrują funkcje we wspólnym targecie `tests/it`, więc łamią globalnie unikalną ścieżkę
`AGENTS.md` §2a i mogą zazielenić pusty wybór. Kontrakt wymaga ponadto zmian
`commands/run.rs` oraz vendor-neutralnego `drivers/mod.rs` poza swoim `OWNS`. Nie wolno
naprawiać go rozszerzeniem własności ani uruchamiać po to, żeby zobaczyć znaną wadę.

Świeże **T-127** startuje z wylądowanego T-126 i ma trzy nowe standalone targety. Dodaje
vendor-neutralny `work_key`, izoluje zwykłe kopie pod `<run>/claude/<work-key>`, refleksję pod
`_reflection`, nadpisuje hostile `CLAUDE_CONFIG_DIR` po `env_clear` i dowodzi trzema
prawdziwymi spawnami bez serializacji. Awaria przygotowania katalogu ma odmówić przed
pierwszym procesem, pokazać dokładne zdanie człowiekowi i zapisać je w `run.json`.

## 2026-08-26, 02:16 — T-126 w trunku: prywatna refleksja, prawdziwy Stop i trwały receipt

**T-126 · zielone · 1 godz. 12 min 08 s do końca lądowania · $0,00 widoczne.** Dwie
porównywalnie zapisane tury Codeksa zużyły łącznie co najmniej 38,75 mln tokenów wejścia
(37,85 mln z cache) i 109,4 tys. wyjścia; recenzja, plan oraz wykonanie naprawy nie mają
wspólnego paragonu tokenów. Enforced `before` uczciwie uruchomiło cztery standalone targety:
5/5 kontroli w 4,61 s, czerwone na brakującym zachowaniu.

Pierwsza pełna bramka przeszła **20/20** w 52,68 s. Refleksja dostała osobne evidence,
ustawienia i receipt, wybór w UI doszedł trzema rzeczywistymi drogami, pusty handoff nie
uruchamiał tury, późny Stop sprzątał prawdziwą grupę, a budżet należał do skonfigurowanego
klona zamiast nazwy modelu. Recenzent znalazł dwie zasadne uwagi medium. Pierwsza była luką
wyroczni AC-4: test dowodził stanu ręcznie złożonego klona, lecz nie wykonywał produkcyjnej
funkcji `reflection_driver`; inspekcja potwierdziła bieżące
`with_budget(REFLECTION_BUDGET_USD)` i wartość `0.08`, ale końcowy oracle fazy musi jeszcze
zmierzyć tę prawdziwą ścieżkę. Druga była wadą kodu: wynik `GroupProof::Alive` był ignorowany.

Jedyna naprawa w `4251ff6` rozróżnia `Dead` i `Alive`: bez `ESRCH` Stop nie udaje
zakończenia, nie zamyka potencjalnie żywego uchwytu i nie zapisuje fikcyjnej refleksji ani
kosztu. Dwie końcowe bramki gałęzi przeszły **20/20** w 52,44 s oraz 47,36 s.
`integrate.sh` wylądował wyłącznie `task-T-126` jako **`b232a580`**; pełna bramka `main`
przed merge'em przeszła 16/16 w 119,31 s, a po merge'u 16/16 w 237,02 s. Trunk jest czysty,
`TASK.md` nie przeżył lądowania. T-109 pozostaje historycznym, niewykonalnym kontraktem;
następne jest świeże T-127, które przejmuje wyłącznie izolację prywatnego stanu Claude'a.

## 2026-08-26, 00:57 — T-125 ZAMKNIĘTE; T-126 przejmuje H14 bez pięciosekundowej pułapki

**T-125 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 01 min 49 s · $0,00
widoczne.** Dwie porównywalnie zapisane tury Codeksa zużyły co najmniej 28,80 mln tokenów
wejścia i 96,8 tys. wyjścia; recenzja, plan i wykonanie naprawy nie mają wspólnego paragonu
tokenów. Enforced `before` uczciwie certyfikowało cztery nowe targety: 5/5 kontroli w 11,30 s,
a AC-2 padło dopiero po zamontowaniu prawdziwej aplikacji w Chromium.

Pierwsza pełna bramka miała **16/20** w 27,11 s. AC-2 i AC-4 były zielone, ale pełny clippy
odrzucił podobne nazwy, AC-1 nie znalazło oczekiwanej kandydatki, a AC-3 zakończyło się
`ENOENT`. Recenzent znalazł cztery zasadne luki: exact-once wracało przed oknem na opóźniony
duplikat, Stop ufał fikcyjnemu `Dead` bez supervisora, budżet można było rozpoznać po promptcie,
a kompatybilność starego `run.json` sprawdzała tylko brak błędu.

Jedyna naprawa w `6b72e1a` dodała okno obserwacji, neutralne prompty, supervisor-backed Stop
i pełniejsze asercje historii. Autorytatywna bramka pozostała **15/20** w 60,14 s. AC-3 i
AC-4 były zielone. AC-1 skanowało `<home>/memory/notes`, choć `scan_notes` samo dopisuje
`notes/`, więc test czytał `notes/notes`; receipt produktu już miał `kept:1`,
`dropped_without_reason:1` i koszt. Wszystkie sześć scen AC-2 czekało stałe 6 sekund przy
domyślnym limicie Vitest 5 sekund. Pełny clippy znalazł ścisłe porównanie `f64`, a formatter
złą kolejność importów. To są błędy wyroczni po jedynej rundzie, nie mandat do piątej tury.

Gałąź `task-T-125` jest czysta na `6b72e1a`, lecz nie wolno jej lądować ani wznawiać.
Świeże **T-126** startuje z `main`, nie przenosi kodu, commitów, speców ani testów T-125.
Skanuje prawidłowy korzeń `<home>/memory`, polluje pierwsze IPC najwyżej 4 sekundy, potem
obserwuje co najmniej 300 ms ciszy przy jawnym limicie testu ≥15 s, używa prawdziwego PGID i
`ESRCH`, identycznych neutralnych promptów, tolerancji `f64` oraz zachowuje id, status i kroki
starego biegu. T-109 i następca pamięci czekają na wylądowanie T-126; operacyjnie dalej
Codex + Codex.

## 2026-08-25, 23:46 — T-123 ZAMKNIĘTE; T-125 przejmuje H14 z prawdziwym efektem przeglądarki

**T-123 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 53 min 51 s · $0,00 widoczne.**
Dwie porównywalnie zapisane tury Codeksa zużyły co najmniej 40,65 mln tokenów wejścia i
91,7 tys. wyjścia; recenzja, plan i wykonanie naprawy nie mają wspólnego paragonu tokenów.
To jest wolumen, który przy płatnym cenniku zewnętrznym przekroczyłby próg ręcznej uwagi;
bieżący harness raportuje jednak koszt widoczny $0,00, więc nie wolno dopisać zmyślonej kwoty.
Enforced `before` uczciwie certyfikowało cztery nowe targety: 5/5 kontroli w 1,21 s.

Pierwsza pełna bramka przeszła **20/20** w 49,48 s. Prywatna refleksja miała osobne evidence,
ustawienia i budżet; zwykły krok zachował fizyczny UUID, rachunek był addytywny, puste
przekazanie nie uruchamiało tury, a tryb klona — nie nazwa modelu — przydzielał limit.
Recenzent znalazł jednak dwie zasadne luki medium. Stop był sprawdzany wyłącznie zanim zwykły
krok skończy scheduler, więc nie dowodził anulowania już żywej refleksji. Frontendowy AC-2
bezpośrednio wołał `requestRun()` i `launchRequested()`: `renderToStaticMarkup` nie uruchamia
efektów, zatem jedyny produkcyjny konsument żądania edytora w
`useSyncExternalStore`/`useEffect` pozostawał niewykonany mimo zielonego testu.

Jedyna naprawa w commicie `98945a2` poprawiła pierwszy defekt: późny Stop anuluje
`AgentHandle` prywatnej tury i nowy scenariusz AC-3 przeszedł 5/5. Nie wolno było uczciwie
załatać drugiego bez efekt-capable przyrządu. Naprawa rozbudowała ponadto produkcyjne
`a_short_turn_about` do 116 wierszy. Ostatnia, autorytatywna bramka miała **19/20** w 41,78 s:
wszystkie AC, pełne testy, scope, format i typy były zielone, ale `full-clippy` odrzucił limit
100 wierszy. Po jednej rundzie nie ma kolejnej tury ani ręcznej poprawki.

Gałąź `task-T-123` jest czysta na `98945a2`, lecz nie wolno jej lądować ani wznawiać.
Odbudowywalny target usunięto (5,1 GiB według Cargo); źródła, branch i paragony pozostały.
Świeże **T-125** startuje z `main`, nie przenosi kodu/speców T-123 i przejmuje H14. Nowe AC-2
używa istniejącego `e2e/harness.ts`, prawdziwego Chromium, widocznego checkboxa, Entera w
`/run` oraz prawdziwego kliknięcia Run w edytorze; dopiero zamontowany efekt może wyemitować
`run_workflow`. AC-3 czeka ze Stopem na żywy proces refleksji i dowodzi jego śmierci, a kontrakt
od początku ogranicza dotknięte funkcje produkcyjne do 100 wierszy. T-109 i T-104 czekają na
wylądowanie T-125; operacyjnie dalej Codex + Codex.

## 2026-08-25, 22:45 — T-124 w trunku: pełna auto-pamięć i trwały atomowy persist

**T-124 · zielone · 24 min 12 s · $0,00 widoczne.** Dwie porównywalnie zapisane tury
Codeksa zużyły co najmniej 6,93 mln tokenów wejścia i 41,3 tys. wyjścia; recenzja, plan i
wykonanie naprawy nie mają wspólnego paragonu tokenów. Enforced `before` uczciwie
certyfikowało trzy nowe targety: 4/4 kontroli w 0,57 s.

Pierwsza pełna bramka przeszła 19/19 w 45,48 s. Auto-pamięć skończonego kroku zachowuje
`ThisAgent`, nazwę właściciela, cały pierwszy akapit, dokładne źródłowe body i `Why`; writer
składa front matter z body w jednym same-directory tempie, a błąd nie zmienia ani starego
pliku, ani pełnego listingu. Read-only plik docelowy przy zapisywalnym katalogu dowodzi, że
zwykły copy-over nie może udawać podmiany.

Recenzent znalazł dwie zasadne granice. Medium: stan końcowy AC-3 nie obserwuje hipotetycznego
mikro-okna `unlink → create`, choć bieżący kod naprawdę używa `NamedTempFile::persist` i nie
ma jawnego remove. Low: `sync_all` tempa plus rename nie utrwala jeszcze wpisu katalogowego
po crashu. Planner zaklasyfikował pierwsze jako ograniczenie wyroczni, nie defekt bieżącej
implementacji; naprawa nie maskowała go grepem ani nową final-state asercją. Drugi problem
naprawił commit `da7827f`: po udanym `persist` otwiera rodzica i propaguje `sync_all` katalogu.
Dwie końcowe bramki przeszły 19/19 w 41,86 s oraz 39,85 s. Ograniczenie AC-3 pozostaje jawne:
test odrzuca copy-over, ale sam nie jest pełnym dowodem braku chwilowego unlinku.

`integrate.sh` wylądował wyłącznie `task-T-124` jako **`63f4246`**. Pełna bramka `main`
przed merge'em przeszła 16/16 w 96,68 s, a po merge'u 16/16 w 176,61 s. Trunk jest czysty,
`TASK.md` nie przeżył lądowania. Odbudowywalny target worktree usunięto (4,9 GiB według
Cargo). Następne jest T-123 przez Codex + Codex.

## 2026-08-25, 22:08 — T-122 ZAMKNIĘTE; T-124 przejmuje H15 z mocniejszą wyrocznią

**T-122 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 23 min 17 s · $0,00 widoczne.**
Dwie porównywalnie zapisane tury Codeksa zużyły co najmniej 7,44 mln tokenów wejścia i
38,3 tys. wyjścia; recenzja, plan i wykonanie naprawy nie mają wspólnego paragonu tokenów.
Enforced `before` uczciwie certyfikowało oba nowe targety: 3/3 kontroli w 0,39 s.

Pierwsza pełna bramka miała **17/18** w 46,57 s. Oba AC, pełne testy, scope, format i typy
były zielone; `full-clippy` odrzucił infallible `draft() -> Result<NoteDraft>` w nowym teście.
Recenzent zgłosił jedną zasadną uwagę medium: AC-2 dowodziło błędu, retry, bajtów i listingu,
ale mogło przepuścić temp-then-copy-over zamiast atomowego rename. Inspekcja bieżącej
implementacji potwierdziła prawidłowe same-directory temp + `sync_all` + `persist`, lecz luka
wyroczni pozostała prawdziwa.

Jedyna naprawa usunęła pierwszy lint bez zmiany produkcji lub asercji, po czym pełny clippy
odsłonił drugi taki sam defekt: infallible `fake_drivers() -> Result<Drivers>` w drugim nowym
teście. Pierwsza bramka po naprawie miała 17/18 w 41,31 s i zielony `full-test`. Ostatnia,
autorytatywna bramka miała **16/18** w 40,54 s: ten sam lint oraz wtórne `ENOSPC` frontendu.
Brak miejsca nie zmienia diagnozy, bo wcześniejsza pełna suita przeszła; po jedynej rundzie
nie ma piątej tury.

Gałąź `task-T-122` jest czysta na `b8f01ca`, ale nie wolno jej lądować ani wznawiać. Usunięto
wyłącznie odbudowywalne targety Cargo czystych, zakończonych worktree (31,0 GiB według Cargo);
źródła, gałęzie i paragony zostały zachowane, a wolne miejsce wzrosło z 123 MiB do 23 GiB.
Świeże **T-124** przejmuje cały H15 bez przenoszenia kodu/speców: fallible helpery mają
`Result`, infallible konkretny typ, a osobna mutacja read-only target + writable parent
odróżnia atomowy persist/rename od copy-over. T-123 zależy teraz od T-124. Operacyjnie dalej
Codex + Codex.

## 2026-08-25, 21:24 — T-121 w trunku: dokładny i atomowy snapshot Store

**T-121 · zielone · 25 min 24 s · $0,00 widoczne.** Dwie porównywalnie zapisane tury
Codeksa zużyły co najmniej 5,33 mln tokenów wejścia i 37,2 tys. wyjścia; recenzja, plan i
wykonanie naprawy nie mają wspólnego paragonu tokenów. Enforced `before` uczciwie
certyfikowało oba nowe targety: 3/3 kontroli w 0,36 s.

Pierwsza pełna bramka przeszła 18/18 w 45,35 s. Recenzent znalazł jednak dwie zasadne luki
oracle: fixture exact-multiset nie zawierała dwóch identycznych eventów, a rollback był
obserwowany dopiero po zwróceniu błędu. Jedyna naprawa była test-only: dodała identyczny
duplikat pełnej krotki oraz współbieżnego czytelnika, który podczas zablokowanej wymiany może
widzieć wyłącznie cały stary snapshot. Produkcyjna implementacja pozostała jednym jobem
jedynego writera i jedną transakcją. Dwie końcowe bramki przeszły 18/18 w 39,26 s oraz
44,02 s.

`integrate.sh` wylądował wyłącznie `task-T-121` jako **`968d239`**. Pełna bramka `main`
przed merge'em przeszła 16/16 w 92,93 s, a po merge'u 16/16 w 171,96 s. Trunk jest czysty,
`TASK.md` nie przeżył lądowania. Następne jest T-122 przez Codex + Codex.

## 2026-08-25, 20:45 — T-120 ZAMKNIĘTE; niezależne cele rozdzielone na T-121…T-123

**T-120 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 55 min 12 s · $0,00 widoczne.**
Dwie porównywalnie zapisane tury Codeksa zużyły co najmniej 26,27 mln tokenów wejścia i
72,3 tys. wyjścia; recenzja, plan i wykonanie naprawy nie mają wspólnego paragonu tokenów.
Enforced `before` uczciwie certyfikowało sześć nowych targetów: 7/7 kontroli w 1,27 s.

Pierwsza pełna bramka miała **15/22** w 48,96 s. Zielone były AC-1 i AC-5, cały scope,
format, typy i granice. Czerwone pokazywały niewykonaną atomową wymianę Store, martwy toggle,
brak semantycznego `left_nothing`, pozostawiony `todo!` atomowego writera, dwa test-only szwy
oraz regresje istniejących dubli refleksji. Recenzent zgłosił trzy uwagi: low o czerwonym
paragonie było wyłącznie findingiem procesu; medium o braku prawdziwego rerenderu checkboxa
i medium o braku końcowych bajtów po udanym retry były zasadne. Planner uwzględnił obie.

Naprawa wylądowała w czterech commitach gałęzi: atomowa wymiana snapshotu, prawdziwy stan
Startu, semantyczne pominięcie pustego handoffu oraz atomowy writer pełnego Markdownu.
Końcowa, autorytatywna bramka na czystym `bde6e8a` miała **19/22** w 17,19 s. Zielone były
AC-1 i AC-3…AC-6, pełny clippy, format, typy, wiring i wszystkie pozostałe quick checks.

Pozostałe trzy czerwienie rozstrzygają kontrakt, nie uzasadniają piątej tury. AC-2 sortowało
odczytane eventy po `body`, ale expected zachowywało kolejność wejściową — poprawny dokładny
snapshot przegrywał na odwróconych dwóch wierszach. `quick-scope` wykazało prawdziwego
właściciela wspólnego stanu `/run`: `src/sections/run/index.tsx` było poza `OWNS`. `full-test`
ujawniło dwa osobne problemy: dozwolone duble z `reflecting()` nie dostały wymaganych
`with_settings`/`with_evidence`/budżetu, a auto-pamięć pojedynczego kroku została błędnie
awansowana z zamrożonego `ThisAgent + agent` do `ThisProject`. Refleksja całego biegu jest
projektowa; prywatna notatka jednego agenta nie.

Gałęzi `task-T-120` nie wolno lądować ani wznawiać; `main` nie dostał jej kodu. Żeby kolejne
zadanie nie mieszało trzech niezależnych domen w jednej rundzie naprawy, jawny mandat
właściciela na cały plan rozdziela świeże zastępstwo: **T-121** ląduje atomowy Store z
order-independent exact multiset, **T-122** zachowuje pełny Markdown i `ThisAgent` atomowo,
a **T-123** ląduje prywatną refleksję, prawdziwy rerender i komplet tras z `index.tsx` w
`OWNS`. Każde startuje ze świeżego trunka i nie przenosi kodu, commitów ani speców T-120.
Kolejność: T-121 → T-122 → T-123 → T-109 → T-104; operacyjnie Codex + Codex.

## 2026-08-25, 19:43 — T-119 ZAMKNIĘTE: fizyczny UUID, zakres i higiena pełnej bramki

**T-119 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 06 min 19 s · $0,00
widoczne.** Dwie porównywalnie zapisane części biegu zużyły co najmniej 21,12 mln tokenów
wejścia i 78,8 tys. wyjścia; recenzja, plan i wykonanie naprawy nie zostawiły kompletnego,
porównywalnego paragonu. Enforced `before` uczciwie certyfikowało sześć nowych targetów:
7/7 kontroli w 1,29 s.

Pierwsza pełna bramka miała **12/22** w 53,16 s: szkielety zachowania pozostały niewykonane.
Recenzent znalazł trzy zasadne luki wyroczni: AC-3 nie dowodziło domyślnego zaznaczenia przed
pierwszą zmianą, AC-2 wymagało nowego artefaktu bez jawnej nieobecności starego, a AC-6
zgadywało nazwy plików tymczasowych zamiast porównać pełny listing katalogu. Planner poprawnie
przeniósł te uwagi do jedynej naprawy, lecz wskazał też trzy produkcyjne trasy Startu poza
zamrożonym `OWNS`; orchestrator jawnie ostrzegł, że nie wolno ich dotknąć.

Naprawa doprowadziła AC-2…AC-6 do zieleni. Końcowa bramka na czystym `dcf0d5c` miała
**17/22** w 20,70 s. AC-1 użyło logicznego klucza workflow `build` jako nazwy pliku evidence,
choć wylądowany kontrakt `run_evidence_reaches_the_product.rs` używa fizycznego UUID kroku z
`run.json`. Pełny clippy odrzucił 123-wierszową funkcję nowego testu, formatter odrzucił nowy
test TS, a `quick-scope` wykazał zmiany `launch.ts`, `requested-launch.ts` i `run-command.ts`
poza `OWNS`. `full-test` agregował błąd AC-1; pozostałe testy systemu były zielone.

Gałęzi `task-T-119` nie wolno lądować ani wznawiać; `main` nie dostał jej kodu. Jawna zgoda
właściciela na cały plan autoryzuje świeże **T-120** z sześcioma globalnie unikalnymi
targetami. Nowy kontrakt wyprowadza oczekiwane evidence z fizycznego UUID w prawdziwym
`run.json`, obejmuje wszystkie trzy trasy Startu w `OWNS`, wymaga formattera, ogranicza każdą
funkcję nowego testu rustowego do 90 wierszy i od początku zamyka wszystkie trzy uwagi
recenzenta. Nie przenosi commitów, implementacji, speców ani testów z T-119. T-120 musi
wylądować przed T-109 i T-104; operacyjna para pozostaje Codex + Codex.

## 2026-08-25, 18:35 — właściciel zatwierdził cały pozostały plan i zastępstwo T-118

Jawne „wykonaj cały plan, możesz modyfikować zadania, masz god mode” uruchamia wyjątek
authoringu dla **T-119** i wszystkich koniecznych świeżych zastępstw. Kod produkcyjny nadal
idzie wyłącznie przez `ship-task.sh`, a każda zielona gałąź osobno przez `integrate.sh`.
Operacyjna para pozostaje Codex + Codex.

T-119 startuje z czystego `main` i nie przenosi commitów, implementacji, speców ani testów z
`task-T-118`. Sześć nowych targetów wpisuje od początku cztery uwagi recenzenta T-118 oraz
poprawia jego błędną wyrocznię: AC-2 zmienia snapshot i wymusza rollback w połowie wymiany;
AC-3 używa handlera toggle'a znalezionego w prawdziwym drzewie `Start`, a następnie uruchamia
przycisk, `/run` i żądanie edytora; AC-4 składa kanoniczne nagłówki wyłącznie przez
`Section::name()`; AC-5 daje zwykłemu krokowi dokładnie model refleksji; AC-6 dowodzi
atomowości przez awarię tworzenia pliku tymczasowego i nietknięte stare bajty. T-119 musi
wylądować przed T-109 i T-104.

## 2026-08-25, 18:24 — T-118 ZAMKNIĘTE: druga czerwień na błędnej wyroczni AC-4

**T-118 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 12 min 07 s · $0,00
widoczne.** Dwie strukturalnie zapisane tury Codeksa zużyły co najmniej 30,66 mln tokenów
wejścia i 92,9 tys. wyjścia; recenzja, plan i wykonanie naprawy nie zostawiły porównywalnego
paragonu tokenów. Enforced `before` uczciwie certyfikowało sześć nowych, samodzielnych plików
testowych: 7/7 kontroli, 1,29 s.

Pierwsza pełna bramka miała **16/22** w 22,27 s. Zielone były AC-1, AC-2 i AC-5; czerwone
wykazały niewpięty atomowy writer, martwy `ReflectionToggle`, brak semantycznego filtrowania
`left_nothing` i regresje istniejących dubli refleksji. Recenzent znalazł dodatkowo cztery
zasadne luki wyroczni: rebuild mógł być no-opem, test Startu nie obejmował `/run` i edytora,
budżet można było przypisać do modelu zamiast trybu refleksji, a końcowe bajty notatki nie
dowodziły atomowego zapisu. Planner przekazał wszystkie te punkty jedynej naprawie.

Naprawa wzmocniła AC-2, AC-3, AC-5 i AC-6, podpięła produkcyjne drogi i usunęła wszystkie
pierwotne czerwienie. Końcowa bramka na czystym `c3ff52f` miała **20/22** w 19,74 s:
zielone były pełny clippy, wszystkie quick checks oraz AC-1, AC-2, AC-3, AC-5 i AC-6.
`full-test` i AC-4 powtarzają ten sam pojedynczy defekt nowego testu.

Wyrocznia AC-4 żąda dosłownie nagłówków `What changed / Decisions / Open questions`, choć
autorytatywny, wylądowany format handoffu w `memory/handoff.rs` to
`Answer / Evidence / Open`. Sam kontrakt T-118 wymagał trwałej nazwanej informacji i
semantyki `left_nothing`, nie tych starych nazw. Zmiana produkcji pod błędną asercję byłaby
oszustwem; zmiana autorytatywnego formatu wymagałaby pliku poza `OWNS`. Po zużyciu jedynej
rundy nie wolno też ręcznie poprawić testu i ponowić bramki. Branch `task-T-118` pozostaje
czysty i **nie może wylądować**; `main` nie dostał jego kodu. Kontynuacja wymaga decyzji
właściciela o świeżym zastępstwie z nowymi globalnie unikalnymi wyroczniami opartymi o
kanoniczne `Answer / Evidence / Open`.

## 2026-08-25, 17:08 — właściciel zatwierdził pełne zastępstwo T-117

Po zamknięciu T-117 właściciel polecił kontynuować. Nowy kontrakt **T-118** startuje ze
świeżego `main`; nie przenosi commitów, implementacji, speców ani testów z
`task-T-117`. Ma sześć nowych, globalnie unikalnych ścieżek kryteriów i musi wylądować
przed T-109 oraz T-104.

Kontrakt zamyka wszystkie trzy wady pierwszej bramki T-117 i uwagę recenzenta. Tryb klona
drivera ma być jawny, więc refleksji nie wolno rozpoznawać po tekście promptu, modelu,
kolejności ani nazwie pliku; zwykły krok i refleksja zachowują równocześnie własne dowody.
Wyrocznia frontendu przechodzi przez prawdziwy element `ReflectionToggle` znaleziony w
drzewie zwróconym przez produkcyjny `Start`, a nie przez osobny helper lub setter. Udany
pusty krok nadal zapisuje prawdziwy handoff `left_nothing`; dopiero refleksja rozpoznaje tę
semantykę i nie uruchamia płatnego procesu. Zapis pełnego Markdownu pamięci ma należeć do
`memory::notes` i być atomowy razem z front matter, bez ponownego otwierania pliku w
`run.rs`. Operacyjna para pozostaje **Codex + Codex**.

## 2026-08-25, 16:43 — T-117 ZAMKNIĘTE: naprawę przerwał pełny dysk

**T-117 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 04 min 13 s · $0,00
widoczne.** Cztery ukończone tury Codeksa zapisały co najmniej 33,73 mln tokenów wejścia
i 109,2 tys. wyjścia; piąta, przerwana tura wykonawcza nie zdążyła zapisać paragonu. Enforced
`before` uczciwie certyfikowało wszystkie sześć nowych AC jako runtime-red: 7/7 kontroli,
1,38 s.

Pierwsza pełna bramka miała **19/22** w 70,31 s. Zielone były AC-2…AC-6, pełny zakres, typy
i wszystkie quick checks. AC-1 wykryło realną regresję: zwykły krok stracił dotychczasową
ścieżkę `logs/agent-<step>.*`. `full-clippy` wykrył zakazane `panic!` w helperze nowego testu
AC-6. `full-test` niosło oba problemy i regresję T-114 wprowadzoną przez T-117: udany pusty
krok przestał zostawiać prawdziwy handoff `left_nothing`.

Recenzent zgłosił dwie uwagi. Zasadna uwaga medium wykazała, że AC-3 woła handler z osobno
wyeksportowanego `reflectionCheckbox()`, nie z kontrolki należącej do drzewa `Start`; martwy
checkbox na ekranie mógłby więc przejść. Uwaga low o braku refleksji w SQLite nie zmienia
kontraktu: prawdą jest blok w `run.json`, SQLite pozostaje odtwarzalnym indeksem, a wyrocznia
już pilnuje bajtów pliku i stabilności wszystkich czterech tabel.

Planner jedynej naprawy prawidłowo rozpisał trzy defekty kodu/testu oraz wzmocnienie AC-3.
Wykonawca zaczął od odczytu kontraktu i źródeł, ale o 16:39 dysk osiągnął `ENOSPC`. Codex nie
mógł zapisać własnego rollouta ani stdout i zakończył paniką `StorageFull` **przed pierwszą
zmianą**. Końcowa próba bramki nie jest wynikiem produktu: nie mogła utworzyć heredoców,
artefaktów Cargo, pliku Vitesta, `runs/.last.tmp` ani `assertions-now.tsv`. Branch pozostał
czysty na `04d0801`; nie ma commita naprawy. Harness zużył jednak cztery tury i jawnie odmawia
piątej, więc T-117 nie wolno wznawiać ani lądować.

Po biegu `cargo clean` usunęło wyłącznie odbudowywalne cache `target/` z zamkniętych worktree
T-117, T-116 i T-103 (raportowane łącznie 15,7 GiB); źródła i paragony pozostały. Wolne miejsce
wzrosło ze 119 MiB do 12 GiB. `main` nie dostał kodu T-117 i pozostaje czysty. Uczciwa
kontynuacja wymaga świeżego zastępstwa z nowymi globalnie unikalnymi specami; nie wolno
przenosić commitów, implementacji ani testów z `task-T-117`.

## 2026-08-25, 15:29 — właściciel zatwierdził uczciwe zastępstwo T-116

Po zamknięciu T-116 właściciel polecił kontynuować. Nowy kontrakt **T-117** startuje ze
świeżego `main` i nie przenosi commitów, implementacji, speców ani testów z `task-T-116`.
`OWNS` obejmuje teraz dokładnie brakującą drogę idempotentnej odbudowy przez
`src-tauri/src/store/mod.rs` i jedynego pisarza w `src-tauri/src/store/writer.rs`.

Sześć nowych, globalnie unikalnych wyroczni usuwa wadliwy setup zwykłego Markdownu i od
początku zamyka cztery uwagi recenzenta: każdy błąd twardego opakowania musi zostawić
`reflection.ran == false`; frontend woła prawdziwy `onChange` widocznego checkboxa; brak
przekazania jest udanym pustym krokiem, nie krokiem `failed`; katalog dowodów odmawia także
dodatkowych plików `reflection*`. Ponowny `Store::rebuild_from` tego samego biegu musi przejść
dwa razy bez duplikatów, bez zmiany `run.json` i bez `INSERT OR IGNORE`. Operacyjna para
pozostaje Codex + Codex na osobnych modelach. T-117 musi wylądować przed T-109 i T-104.

## 2026-08-25, 15:05 — T-116 ZAMKNIĘTE po drugiej czerwieni

**T-116 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 02 min 43 s · $0,00 widoczne.**
Etapy Codeksa nie zapisały kompletnej wyceny, więc to wyłącznie koszt widoczny. Enforced
`before` uczciwie certyfikowało sześć nowych AC jako runtime-red. Pierwsza i druga pełna
bramka miały **19/22**; końcowa trwała 19,97 s. Zielone były AC-1, AC-3, AC-4 i AC-5,
pełny clippy, zakres, typy oraz wszystkie pozostałe szybkie kontrole. Czerwone pozostały
AC-2, AC-6 i agregujący je `full-test`.

AC-2 ujawniło kolejny rzeczywisty brak zakresu. Wyrocznia odbudowuje ten sam bieg ponownie,
a `Store::rebuild_from` próbuje drugi raz wstawić ten sam `runs.id` i dostaje SQLite 1555
`UNIQUE constraint failed: runs.id`. Planner naprawy uznał idempotentną odbudowę indeksu za
defekt kodu i wskazał najmniejszą zmianę w `src-tauri/src/store/mod.rs`. Tego pliku nie ma w
`OWNS` T-116; zmiana identyfikatora, przepisywanie `run.json` albo `INSERT OR IGNORE` bez
rozstrzygnięcia zdarzeń byłyby obejściem pod test.

AC-6 ma niezależny defekt zamrożonej wyroczni. Helper `body_of_text` woła
`FrontMatter::split` na zwykłym pliku Markdown bez front matter i kończy się
`NoFrontMatter { path: "" }`, choć produkcyjny kolektor poprawnie traktuje taki plik jako
ciało od bajtu zero. Dodanie sztucznego front matter zmieniłoby przypadek, a poprawa helpera
po certyfikacji `before` byłaby zmianą kryterium. Planner nazwał to wprost defektem testu;
wykonawca zgodnie z regułą nie zrobił częściowej naprawy ani commita.

Recenzent wskazał ponadto cztery nierozstrzygnięte luki oracle: brak asercji `ran:false` dla
każdego brakującego wrappera, ominięcie prawdziwego handlera widocznego checkboxa, przypadek
„bez handoffu” zbudowany z porażki zamiast udanego pustego wyniku oraz brak odmowy dodatkowych
plików refleksji. Wszystkie uwagi są zasadne i muszą wejść do następnego kontraktu od początku.

Gałąź `task-T-116` jest czysta na `af18576`; `quick-scope` było zielone, lecz gałęzi nie wolno
lądować. `main` nie dostał jej kodu i pozostaje czysty. Faza 7 stoi przed T-109. Uczciwa
kontynuacja wymaga kolejnego pełnego zastępstwa z `store/mod.rs` w `OWNS`, sześcioma nowymi
ścieżkami testów, poprawną fiksturą Markdown przed `before` i czterema domkniętymi lukami
recenzenta; nie wolno przenosić commitów, implementacji ani speców z `task-T-116`.

## 2026-08-25, 13:59 — właściciel zatwierdził pełne zastępstwo T-103

Jawne „rób, masz pozwolenie na wszystko” właściciela uruchamia wyjątek authoringu dla
**T-116**. Nowy kontrakt startuje z czystego `main`, nie przenosi commitów, implementacji ani
speców z `task-T-103` i obejmuje oba brakujące pliki: `src-tauri/src/evidence.rs` oraz
`src/sections/run/io.ts`. Ma sześć nowych globalnie unikalnych ścieżek testów.

T-116 wzmacnia też dwie luki ujawnione przez drugą czerwień: prawdziwy panel Startu pokazuje
działający, domyślnie włączony przełącznik refleksji, a oba istniejące opt-in duble zachowują
swoje stare asercje po dołożeniu prywatnych ustawień, dowodów i sufitu ceny. Operacyjna para
pozostaje Codex + Codex na osobnych modelach. T-116 musi wylądować przed T-109 i T-104.

## 2026-08-25, 13:37 — T-103 ZAMKNIĘTE: kontrakt wymaga dwóch plików poza OWNS

**T-103 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 15 min 25 s · $0,00 widoczne.**
Etapy Codeksa nie zapisały kompletnej wyceny, więc to wyłącznie koszt widoczny. Enforced
`before` uczciwie certyfikowało pięć nowych AC jako runtime-red. Po jedynej rundzie naprawy
końcowa pełna bramka miała **19/21 w 15,93 s**: wszystkie AC, clippy, typy i kontrole
podpięcia były zielone; czerwone pozostały `full-test` i `quick-scope`.

Kontraktu nie da się uczciwie wykonać w zamrożonym `OWNS`. AC-1 wymaga dokładnych artefaktów
`logs/reflection.jsonl`, `logs/reflection.stderr.log` i `logs/reflection.input.json`, ale
istniejący `EvidenceTarget` umie nazwać tylko dowody kroku grafu albo rozmowy Lead. Jawna
tożsamość i konstruktor refleksji muszą powstać w `src-tauri/src/evidence.rs`, którego nie ma
w `OWNS`. Pierwszy wykonawca wykrył to przed implementacją i poprawnie odmówił kopiowania,
symlinków oraz pustych plików jako obejścia pod test. AC-3 niezależnie wymaga, żeby produkcyjny
Start wysłał nazwany klucz `reflectionEnabled`: Tauri deserializuje argumenty przed wejściem
do komendy i pominięty `Option<bool>` odrzuca wywołanie. Ta krawędź mieszka w
`src/sections/run/io.ts`, również poza `OWNS`.

Wykonawca naprawy mimo tego zmienił oba pliki poza zakresem. Dodatkowo nowe obowiązkowe
opakowania `with_settings`, `with_evidence` i limitu ceny sprawiły, że istniejący dubler,
który jawnie podaje wyłącznie szew `reflecting()`, przestał wykonywać turę. Stary oracle
`a_run_that_handed_nothing_on_is_never_asked` dostał zero wywołań zamiast jednego. To jest
rzeczywista regresja implementacji, ale po drugiej czerwieni nie ma kolejnej rundy naprawy.

Zgodnie z regułą fazy zakresu nie rozszerzono i zadania nie przepisano. Gałąź `task-T-103`
pozostaje czysta na `6db6091`, lecz nie wolno jej lądować; `main` nie dostał żadnego jej kodu
i pozostaje czysty. Faza 7 stoi przed T-104. Uczciwa kontynuacja wymaga nowego zadania
zastępczego z pełnym `OWNS` i nowymi, globalnie unikalnymi ścieżkami testów.

## 2026-08-25, 12:00 — T-115 w trunku po dwóch jawnie autoryzowanych poprawkach testowych

**T-115 · zielone · 57 min 05 s właściwego biegu harnessu + ręczne domknięcie lintów ·
$0,00 widoczne.** Etapy Codeksa nie zapisały kompletnej wyceny ani księgi użycia, więc to
wyłącznie koszt widoczny. Mocne cztery AC pozostały bez zmian: nierówne kolumny cen są
sprawdzane osobno dla Sol/Terra/Luna, ekran sumuje dwa różne koszty, nieznany model zachowuje
tokeny bez `$0.00`, a Codex dostaje otwieralne pełne ścieżki handoffów.

Po zielonej pierwszej bramce 20/20 recenzent znalazł dwa średnie defekty, które jedyna runda
naprawy domknęła z regresjami: model dociera teraz także przez App Server do wspólnego dekodera
cen (`9573327`), a uwaga o nieznanej cenie jest kojarzona po stabilnym kluczu kroku zamiast po
nieunikalnej nazwie (`3396e6f`). Naprawa zostawiła jedynie deterministyczne lity we własnym
nowym teście. Właściciel dwa razy jawnie dopuścił test-only domknięcie poza zakończonym grafem:
przeniesienie stałej przed instrukcje (`0c79213`) i zamianę dwóch `assert!(false)` na
równoważne `Result::Err` (`9c6635d`). Nie zmieniono produkcji, kryterium ani siły asercji.

Po pierwszym ręcznym commicie `cargo check --all-targets --keep-going` był zielony, a pełna
bramka miała 19/20 i ujawniła drugi lint. Po drugim commitcie `cargo check` znów był zielony,
a pełna bramka przeszła **20/20 w 65,73 s**. `integrate.sh` wylądował wyłącznie
`task-T-115` jako **`118c876`**: bramka main przed merge'em przeszła **16/16 w 90,33 s**,
a po merge'u **16/16 w 171,77 s**. Drzewo jest czyste, `TASK.md` nie przeżył lądowania.
Następne jest T-103, przez Codex + Codex.

## 2026-08-25, 11:08 — T-115 czerwone po naprawie; wyłącznie testowy lint wymaga decyzji

**T-115 · czerwone / NIEWYLĄDOWANE · 57 min 05 s · $0,00 widoczne.** Etapy Codeksa nie
zapisały kompletnej wyceny ani księgi użycia, więc to wyłącznie koszt widoczny. Enforced
`before` uczciwie certyfikowało cztery nowe AC jako runtime-red. Odczyt zamrożonych speców
potwierdził, że naprawiają obie luki T-102: każdy znany model dostaje nierówne 10k/5k/20k,
a prawdziwy ekran odróżnia sumę `$1.20` od obu operandów `$0.41` i `$0.79`.

Pierwsza pełna bramka przeszła **20/20 w 42,39 s**. Recenzent tego samego vendora na osobnym
modelu gpt-5.5 znalazł jednak dwa średnie defekty kodu: ścieżka App Servera nie niosła modelu
do wspólnego dekodera ceny, a uwagi o nieznanej cenie były kojarzone po nieunikalnej nazwie
agenta zamiast po stabilnym kluczu kroku. Jedyna runda naprawy przeniosła model przez App
Server i dodała regresję (`9573327`) oraz przypisała uwagę do prawdziwego klucza kroku z
regresją dwóch równoległych kroków o tej samej nazwie (`3396e6f`). Wszystkie AC i `full-test`
są po naprawie zielone.

Wykonawca naprawy wprowadził jedną deterministyczną czerwień wyłącznie w nowym teście:
`const UNKNOWN_MODEL` w `codex.rs` stoi po instrukcjach i uruchamia
`clippy::items_after_statements`. Dwie końcowe bramki miały przez to **19/20 w 45,26 s** i
**19/20 w 40,88 s**; jedyną porażką był ten sam lint, bez porażki zachowania. Przesunięcie
deklaracji przed instrukcje nie zmienia kryterium ani kodu produkcyjnego, lecz byłoby piątą,
ręczną turą po zamknięciu grafu Harnessu. Zgodnie z AGENTS.md §7 orchestrator zatrzymał się
zamiast robić ją po cichu.

Gałąź `task-T-115` jest czysta na `3396e6f`; `main` nie dostał jej kodu. Faza 7 stoi przed
T-103. Potrzebna jest jawna decyzja właściciela: dopuścić audytowalną, test-only poprawkę
poza grafem, potem `cargo check --all-targets --keep-going` i pełną bramkę, albo zamknąć
T-115 i pisać kolejne zastępstwo.

## 2026-08-25, 02:25 — właściciel zatwierdził pełne zastępstwo T-102

Jawne „ok” właściciela uruchamia wyjątek authoringu dla **T-115**. Nowy kontrakt startuje
z czystego `main`, nie przenosi gałęzi, implementacji ani speców T-102 i ma cztery globalnie
unikalne ścieżki testów. Cennik każdego znanego modelu dostaje nierówne liczniki 10k/5k/20k,
więc zamiana wejścia, cache lub wyjścia jest czerwona; prawdziwy ekran dostaje co najmniej
dwa niezerowe koszty różnych vendorów i musi pokazać ich sumę, nie jeden operand. T-115 musi
wylądować przed T-103. Operacyjna para pozostaje Codex + Codex na osobnych modelach.

## 2026-08-25, 02:24 — T-102 zielone, lecz NIEWYLĄDOWANE; dwie uwagi wyroczni zostały otwarte

**T-102 · formalnie zielone 20/20, lecz NIEWYLĄDOWANE · 38 min 53 s · $0,00 widoczne.**
Etapy Codeksa nie zapisały kompletnej wyceny ani księgi użycia, więc to wyłącznie koszt
widoczny. Enforced `before` uczciwie certyfikowało wszystkie cztery AC jako runtime-red.
Implementacja na czystej gałęzi `task-T-102` kończy się w `7a22629`: wycenia znane modele
Codeksa jako szacunek, zachowuje same tokeny dla nieznanego modelu, pokazuje wydatki na pasku
i wyjaśnia krokom Codeksa, że pliki handoffów leżą poza katalogiem pracy.

Pierwsza pełna bramka miała **19/20**: wszystkie AC i `full-test` były zielone, a jedyną
czerwienią był `clippy::doc_markdown` w komentarzu nowej wyroczni. Recenzent tego samego
vendora na osobnym modelu gpt-5.5 znalazł jednak dwie niezależne luki asercji. Średnia uwaga:
test tabeli daje Terra i Luna dokładnie po milionie tokenów wejścia, cache i wyjścia, więc
zamiana stawek między kolumnami zachowuje tę samą sumę i nadal przechodzi. Niska uwaga: test
prawdziwego ekranu zasila pasek jedną płatną linią, więc implementacja pokazująca pierwszy
albo ostatni koszt zamiast sumy obu vendorów także przechodzi.

Planista poprawnie zaproponował nierówne próbki per model oraz dwa płatne kroki na ekranie,
ale nazwał je „criterion/test defect”. Wykonawca zinterpretował regułę zamrożonego oracle
dosłownie: poprawił wyłącznie lint w `7a22629` i odmówił zmiany obu asercji. Dwie końcowe pełne
bramki były przez to zielone **20/20 w 40,15 s** i **20/20 w 38,42 s**, lecz nie odpowiadają
na drugą opinię. Odczyt pozostałych testów potwierdził, że żadna niezałączona wyrocznia nie
zamyka luk: nierówne tokeny są sprawdzone tylko dla Sol, a wszystkie testy paska używają
jednego płatnego wiersza.

Gałęzi nie wylądowano mimo kodu 0, ponieważ zielone kryterium da się przejść dokładnie tymi
dwoma błędnymi implementacjami. Zmiana zamrożonych speców po certyfikacji `before` albo druga
runda naprawy łamałyby kontrakt Harnessu. `main` pozostaje czysty i bez kodu T-102; faza 7
stoi przed T-103. Uczciwe wyjście to nowe zadanie zastępcze z nowymi globalnie unikalnymi
ścieżkami testów, nierównymi tokenami dla każdego modelu i ekranową sumą co najmniej dwóch
płatnych kroków.

## 2026-08-25, 01:44 — T-101 w trunku

**T-101 · zielone · 47 min 38 s biegu harnessu · $0,00 widoczne.** Etapy Codeksa nie
zapisały kompletnej wyceny ani księgi użycia, więc to wyłącznie koszt widoczny. Enforced
`before` uczciwie certyfikowało wszystkie cztery AC jako runtime-red. Pierwsza tura
implementacji nie zostawiła zmian i pierwsza bramka miała **15/20**: czerwone były cztery AC
oraz agregujący je `full-test`.

Planista naprawy potwierdził trzy rzeczywiste boczne drzwi omijające wspólną politykę porażki.
Jedyna runda naprawy skierowała odmowę kontekstu (`bfeeec9`), zablokowaną trasę (`0fc04c5`)
i sufit budżetu (`b5bf409`) przez `when_this_one_fails`. W rezultacie ustawienia `carry-on`,
`ask-me` i `stop` działają na tych ścieżkach tak samo jak na zwykłej porażce, strumień zgadza
się z książką, potomkowie zatrzymani budżetem mówią o budżecie zamiast o Stopie człowieka,
a `carry-on` przekazuje prawdziwe ostatnie słowa.

Recenzent tego samego vendora na osobnym modelu gpt-5.5 zgłosił tylko niską uwagę o braku
zielonego paragonu przed naprawą. Pierwsza pełna bramka po naprawie miała **19/20**: wszystkie
AC były zielone, a stary test `trigger_editor_writes_safe_file` spoza OWNS dostał przejściowy
`RecvError`/timeout w cleanupie. Końcowa bramka Harnessu, bez żadnej zmiany kodu lub testu,
przeszła **20/20 w 43,07 s**; flake nie został zamaskowany ani naprawiony w tym zadaniu.

`integrate.sh` wylądował wyłącznie `task-T-101` jako **`73ec11c`**. Pełna bramka main przed
merge'em przeszła **16/16 w 88,81 s**, a po merge'u **16/16 w 171,25 s**. Drzewo jest czyste,
`TASK.md` nie przeżył lądowania. Następne jest T-102, przez Codex + Codex.

## 2026-08-25, 00:50 — T-100 w trunku

**T-100 · zielone · 36 min 18 s biegu harnessu · $0,00 widoczne.** Etapy Codeksa nie
zapisały kompletnej wyceny ani księgi użycia, więc `$0,00` oznacza wyłącznie koszt widoczny,
nie oszacowanie ceny. Wymuszone `before` uczciwie certyfikowało cztery kryteria jako runtime-red.
Pierwsza pełna bramka gałęzi przeszła **20/20 w 42,83 s**, a dwie pełne bramki po jedynej
rundzie naprawy przeszły **20/20 w 42,60 s** i **20/20 w 37,23 s**.

Tester pętli dostaje wymagane pole `outcome`; ustrukturyzowana wartość rozstrzyga przed
zgodnościową linią prozy, tester widzi wszystkie wcześniejsze próby implementera, a `run.json`
addytywnie zapisuje wynik każdej rundy. Recenzent tego samego vendora na osobnym modelu gpt-5.5
znalazł rzeczywistą lukę mimo zielonej bramki: parser pola czytał je tylko wewnątrz
`## Answer`, choć wspólny nośnik `key: value` nie ma takiego ograniczenia. Jedyna naprawa
rozszerzyła odczyt na całe ciało i dodała regresję, w której kanoniczne `outcome: pass` poza
Answer wygrywa ze sprzecznym późniejszym markerem (`aacd038`).

`integrate.sh` wylądował wyłącznie `task-T-100` jako **`18e0cd3`**. Pełna bramka main przed
merge'em przeszła **16/16 w 88,58 s**, a po merge'u **16/16 w 162,37 s**. Drzewo jest czyste,
`TASK.md` nie przeżył lądowania. Następne jest T-101, przez Codex + Codex.

## 2026-08-25, 00:09 — T-114 w trunku

**T-114 · zielone · 42 min 32 s biegu harnessu · $0,00 widoczne.** Zapisane tury kontraktu
i implementacji Codeksa zużyły łącznie co najmniej **25,54 mln tokenów wejścia i 68,7 tys.
wyjścia**; osobna recenzja oraz plan i wykonanie naprawy nie zapisały kompletnego użycia ani
wyceny. Wymuszone `before` uczciwie certyfikowało sześć runtime-red speców, a końcowa bramka
gałęzi przeszła dwukrotnie **22/22** (44,11 s i 39,29 s).

Kopie `fresh-copy` mają osobne poprawne refy, a kolizja zakodowanych ogonów jest widocznym
ostrzeżeniem przy zapisie i Problemem przy Starcie przed katalogiem biegu, worktree i spawnem.
Prompt podaje otwieralny adres pełnej kopii bieżącego biegu, zachowując prawdziwą etykietę
zwykłego poprzednika albo pliku przeniesionego z wcześniejszego biegu. Ostatnie `outcome:`
przeżywa limit dokładnie raz, pusta udana odpowiedź jest nazwana, a tylko źródło strzałki
powrotnej musi mieć jedną kopię.

Recenzent samego vendora (osobny model gpt-5.5) znalazł rzeczywistą lukę: rozdęta preambuła
przed `## Answer` nie dzieliła budżetu 8 KB z nagłówkami, wskaźnikami i końcową decyzją.
Jedyna naprawa dodała mocną regresję wymagającą limitu, wszystkich nagłówków, wskaźnika,
jednej decyzji i pełnej kopii bajt w bajt (`43c3a4c`). Pierwsza oficjalna bramka miała
niezależny timeout gotowości starego E2E; `e2e/harness.ts` poza OWNS pozostał nietknięty,
a dwa następne pełne przebiegi były zielone.

`integrate.sh` wylądował wyłącznie `task-T-114` jako **`50ad074`**. Pełna bramka main przed
merge'em przeszła **16/16 w 127,84 s**, a po merge'u **16/16 w 160,77 s**. Drzewo jest czyste,
`TASK.md` nie przeżył lądowania. Następne jest T-100, przez Codex + Codex.

## 2026-08-24, 21:24 — właściciel zatwierdził T-114 i Codex + Codex

Jawne „ok” właściciela uruchamia wyjątek authoringu dla **T-114**, pełnego zastępstwa
zamkniętych T-99/T-112/T-113. Nowy kontrakt startuje z `main`, ma sześć globalnie unikalnych
ścieżek i nie przenosi starej implementacji. Poprawione AC-3 osobno wymaga etykiety zwykłego
poprzednika oraz prawdziwej etykiety pliku przeniesionego z wcześniejszego biegu; obie ścieżki
muszą wskazywać pełną kopię pod katalogiem bieżącego biegu. T-114 musi wylądować przed T-100.

Właściciel jawnie polecił używać dalej **Codex + Codex**, ponieważ kończy się budżet Claude'a.
Recenzja pozostaje osobnym wywołaniem Harnessu, w roli tylko do odczytu i na innym modelu;
ograniczenie samego vendora ma być raportowane, ale nie zastępowane Claude'em bez nowej decyzji.

## 2026-08-24, 21:21 — T-113 czerwone po naprawie; błędny spec AC-3 wymaga decyzji człowieka

**T-113 · czerwone / NIEWYLĄDOWANE · 47 min 12 s od pierwszego startu · $0,00 widoczne.**
Po przełączeniu właściciela na Codex + Codex kontrakt i implementacja zapisały łącznie co
najmniej **22,86 mln tokenów wejścia i 69,0 tys. wyjścia**; recenzja oraz plan i wykonanie
naprawy nie zapisały osobnej ceny ani kompletnego użycia. Wymuszone `before` uczciwie
certyfikowało wszystkie sześć AC jako runtime-red. Implementacja pozostała w OWNS i kończy się
na czystej gałęzi `task-T-113` w `9bd71fa`; trunk nie dostał żadnej jej zmiany.

Pierwsza i druga pełna bramka po jedynej rundzie naprawczej miały **20/22**. Zielone są AC-1,
AC-2, AC-4, AC-5 i AC-6, pełny clippy oraz wszystkie szybkie kontrole. Czerwone są wyłącznie
AC-3 i agregujący tę samą porażkę `full-test`. Działający kod daje czytelnikowi bezwzględny,
otwieralny adres pełnej kopii w katalogu bieżącego biegu, zachowuje względny wskaźnik na dysku,
przenosi adres przy wznowieniu i nie tworzy attachmentu dla krótkiej odpowiedzi.

Powód czerwieni jest w zamrożonym specu kontraktowym, nie w tym zachowaniu. Ten sam helper
asercji wymaga po wznowieniu dokładnej etykiety `what the step before left`, choć istniejące
wyrocznie i produkcyjny model pochodzenia wymagają wtedy `what an earlier run left here`.
Zadanie wymaga dla wznowienia nowego adresu, nie fałszywej informacji o pochodzeniu. Obu
dokładnych równości nie da się spełnić równocześnie. Uzależnienie etykiety od długości tekstu,
przeklasyfikowanie przeniesionego pliku na zwykłego poprzednika albo zmiana speca po
certyfikacji `before` byłyby oszustwem pod test.

Recenzent zgłosił jedną niską uwagę proceduralną: czerwony paragon nie pozwala zweryfikować
zmiany jako gotowej. Planista naprawy wskazał błędne kryterium i zalecił decyzję człowieka;
wykonawca poprawnie zatrzymał się bez zmian i commita. Zgodnie z AGENTS.md §7 oraz regułą
drugiej czerwieni faza 7 stoi przed T-100. T-113 nie wolno landować ani wznawiać bez jawnej
decyzji, czy zastąpić błędny spec nowym kontraktem.

## 2026-08-24, 20:30 — oracle `before` naprawiony; T-113 ma zgodę i nowy kontrakt

Właściciel jawnie autoryzował osobną naprawę harnessu oraz T-113. Commit **`5604c3d`**
zamyka wadę z biegu T-112: `NOT_A_REAL_RED` rozpoznaje każdą numerowaną diagnostykę
kompilatora Rusta oraz końcowe `could not compile`, więc E0308 we wspólnym targetcie nie może
już udawać czerwonego zachowania. Automatyczny selftest pyta funkcję werdyktu, nie tekst regexu:
reprezentatywny E0308 musi dostać „did not RUN”, a runtime'owa panika testu nadal certyfikuje
uczciwe `before`. Nie rozluźniono kryterium ani żadnego istniejącego wyjątku.

Składnia obu plików przeszła, nowy selftest przeszedł dwukrotnie, a pełne pasy Rust i Web były
zielone. Końcowy `harness/guards.sh` na czystym commicie ujawnił osobny, istniejący stan własnej
księgi: **11 strażników zadziałało, 1 (`quick-scope`) chybił, 4 odkryte checki nie mają funkcji
guard** (`before-spec-owns`, `quick-invoke-args`, `quick-tests-listed`, `quick-wired`). Tego
wyniku nie zamaskowano ani nie dopisano wyjątków do commita naprawiającego `before`; wymaga
osobnego rozstrzygnięcia harnessu.

Nowe **T-113** jest pełnym zastępstwem T-99/T-112 z sześcioma globalnie unikalnymi ścieżkami
speców. Zachowuje poprawne refy kopii, żywy adres załącznika, końcowe `outcome:`, sygnał pustej
odpowiedzi i jednoznacznego sędziego. Dodane kryterium liczy planowane klucze `fresh-copy`
tym samym kodowaniem co Git i wymaga widocznej odmowy kolizji `s_2~2` z literalnym `s_2-2`
przez prawdziwy Start — przed katalogiem biegu, drzewem roboczym i pierwszym procesem. Hashowanie
lub losowe przemianowanie nie jest dopuszczonym obejściem. T-99/T-112 pozostają tylko dowodem;
T-100 czeka na wylądowanie T-113.

## 2026-08-24, 19:04 — T-112 ZAMKNIĘTE bez lądowania; zielona bramka była nieważna

**T-112 · formalnie zielone 21/21, lecz ZAMKNIĘTE / NIEWYLĄDOWANE · 1 h 09 min 09 s ·
co najmniej $34,78 widoczne.** Kontrakt Claude'a kosztował **$14,52** i doszedł do limitu
81 tur; implementacja kosztowała **$20,25** w 139 turach. Recenzja Codeksa i wykonanie
naprawy przez Claude'a nie zapisały osobnej ceny. Końcowa bramka przy czystym drzewie miała
21/21 w 36,76 s, a jedyna runda naprawcza mechanicznie rozbiła 102-liniowy test bez zmiany
asercji (`0234a26`). Gałąź nie została wylądowana mimo kodu 0.

Pierwszy powód jest kontraktowy. `branch_for(run, "s_2~2") → s_2-2` oraz gwarancja, że
prawidłowy klucz `s_2-2` przechodzi niezmieniony, wybierają ten sam ref. Loadout akceptuje
ręcznie zapisane identyfikatory bez ograniczenia znaków; workflow z `s_2` w dwóch kopiach i
osobnym `s_2-2` zapisuje się, przechodzi `check_to_run` i odmawia dopiero podczas drugiego
`git worktree add -b` — po rozpoczęciu pracy, wbrew niezmiennikowi 12. Zielone AC-1 nie ćwiczy
tej kolizji. Uczciwe wyjście wymaga nowego kontraktu: rekomendowana jest widoczna odmowa
kolizji zakodowanych refów przed pierwszym procesem, zamiast zmiany istniejących nazw gałęzi.

Drugi powód unieważnia cały paragon `before`. Kontraktowy test AC-3 destrukturyzował
3-elementowy wynik jako dwie wartości. Między commitem kontraktu `cabbfc4` a implementacją
`6820eec` agent musiał poprawić dokładnie dwa takie miejsca. Każde z pięciu AC kompiluje wspólny
target `it`, więc wszystkie wymuszone `before` padły na ten sam E0308, nie na brak zachowania.
Harness nie rozpoznał podpisu błędu kompilacji i błędnie wypisał „red for the right reason”.
To defekt warstwy zaufania: dopóki nie powstanie osobny, autoryzowany commit harnessu z
selftestem, następny kontrakt Rust może dostać ten sam fałszywy certyfikat.

Dodatkowe znalezisko spoza OWNS: `commands/history.rs::branches_of_run` dopasowuje ogon refa
do `tile_key`, więc nową gałąź drugiej kopii `s_2-2` pokaże bez etykiety kroku. Jest to mniejsza
resztka produktu, nie powód do cichego rozszerzenia T-112.

Gałąź `task-T-112` pozostaje niewylądowana na `0234a26`; trunk nie dostał kodu produkcyjnego.
Faza 7 zatrzymuje się przed T-100. Potrzebna jest jawna zgoda właściciela na osobną naprawę
harnessu oraz na nowy kontrakt zastępczy z walidacją kolizji refów.

## 2026-08-24, 17:53 — właściciel zatwierdził zastępcze T-112

Jawne „ok” właściciela na rekomendowany task zastępczy uruchamia wyjątek authoringu wyłącznie
dla kontraktu i dokumentacji. **T-112** zastępuje zamknięte T-99 i nie bierze z jego gałęzi
commitów ani speców. Rozstrzygnięcia są trzy: ref drugiej kopii to poprawne dla Gita `s_2-2`,
trwały handoff zachowuje względny i przenośny wskaźnik, natomiast zmontowany prompt odbiorcy
dostaje bezwzględny adres kopii z bieżącego biegu; sędzią pętli jest źródło strzałki powrotnej.
Pięć nowych ścieżek wyroczni jest globalnie unikalnych. T-112 musi wylądować przed T-100.

## 2026-08-24, 17:31 — T-99 ZAMKNIĘTE, sprzeczne wyrocznie i dwa błędy kontraktu

**T-99 · czerwone / ZAMKNIĘTE · 1 h 04 min 26 s · co najmniej $35,23 widoczne.** Kontrakt
Claude'a kosztował **$13,50** i doszedł do limitu 81 tur; implementacja kosztowała **$21,72**
w 156 turach. Recenzja Codeksa i wykonanie naprawy przez Claude'a nie zapisały osobnej ceny.
Mimo niezerowego wyjścia fazy kontraktu wymuszone `before` uczciwie certyfikowało wszystkie
cztery AC jako czerwone z właściwego powodu.

Pierwsza pełna bramka była czerwona na AC-2, tych samych dwóch przypadkach w `full-test` oraz
pięciu lintach w nowych testach. Jedyna runda naprawcza poprawiła linty bez suppressions
(`225ee2e`) i zapisała bezwzględny wskaźnik pełnej kopii (`777c041`). Ostatnia bramka przy
czystym drzewie miała **19/20 w 14,51 s**: AC-1/AC-2/AC-3/AC-4, clippy i wszystkie szybkie
sprawdzenia były zielone, lecz `full-test` nadal miał dokładnie dwie porażki.

To nie jest zaproszenie do ręcznej naprawy. `src-tauri/tests/it/memory_handoff_cap.rs`, którego
nie ma w OWNS T-99, asertuje dokładną relatywną linię `Moved to attachments/<name>`. AC-2
asertuje dla tej samej linii ścieżkę bezwzględną i otwieralną z dowolnego katalogu. Jedna
wartość nie może spełnić obu równości. Istniejący zielony test wznowienia dodatkowo staje się
fałszywy: `new_run.join(absolute_path)` ignoruje `new_run` i sprawdza plik starego biegu.
Naprawa wymagałaby pliku spoza OWNS oraz decyzji, co bezwzględny adres ma znaczyć po usunięciu
poprzedniego biegu. Zgodnie z regułą fazy nie rozszerzono OWNS i nie osłabiono żadnej wyroczni.

Recenzent wykrył też dwa niezależne błędy tekstu zadania. AC-1 wymaga dokładnej gałęzi
`loadout/<run>/s_2~2`, ale Git zabrania `~` w refach; produkcja poprawnie koduje ją jako
`s_2-2`, a zielony test sprawdza tylko różność i prefiks, więc nie certyfikuje literalnego AC.
AC-4 mówi o **celu** strzałki powrotnej (`link.to`), podczas gdy opis zadania, model pętli,
produkcja i test wskazują kafelek zamykający pętlę (`link.from`); wymuszenie celu zakazałoby
legalnych wielokrotnych wejść do pętli. Zielone AC nie naprawiają tych sprzeczności.

Gałąź `task-T-99` pozostaje niewylądowana na `777c041`; trunk nie dostał żadnej jej zmiany.
Faza 7 zatrzymuje się tutaj zgodnie z regułą drugiej czerwieni i nierozstrzygniętych uwag
recenzenta. T-100 nie został rozpoczęty.

## 2026-08-24, 16:25 — T-111 w trunku

**T-111 · zielone · 41 min 56 s biegu harnessu + jawnie zatwierdzone domknięcie testowe ·
koszt widoczny: brak wyceny.** Zapisane etapy kontraktu i implementacji Codeksa zużyły łącznie
**13,32 mln tokenów wejścia i 41,7 tys. wyjścia**; artefakty naprawy i recenzji Claude'a nie
zapisały kompletnej ceny. Lead Codeksa czyta teraz efektywną konfigurację przed `thread/start`,
wyłącza prywatne MCP w konfiguracji wątku, ponownie włącza wyłącznie zatwierdzone Connections
i odmawia startu, gdy konfiguracji nie da się bezpiecznie zinterpretować. Ten sam encoder klucza
TOML obsługuje identyfikatory z cudzysłowem i znakami sterującymi, a dane prywatnych serwerów
nie trafiają do argv ani evidence.

Recenzent cross-vendor zgłosił pięć uwag. Cztery zakończyły się poprawkami: brak lub `null`
`mcp_servers` oznacza pustą kolekcję, wyrocznia wykrywa też escaped identyfikator, odmowa zatruwa
evidence, a encoder ucieka znaki sterujące. Piątą — czy nakładka per-thread odpowiada żywej
semantyce App Servera — rozstrzygnęły oficjalne typy protokołu i implementacja `ConfigManager`:
mapa `ThreadStartParams.config` jest konwertowana do par TOML i dokładana po nakładkach CLI,
a `mcp_servers.<id>.enabled` jest oficjalnym przełącznikiem. Nie przyjęto jej ani nie odrzucono
na intuicję.

Po jedynej rundzie naprawczej pełna bramka była czerwona wyłącznie dlatego, że nowy test miał
112 linii przy limicie clippy 100. Zgodnie z osobną zgodą właściciela mechanicznie rozbito jego
niezmienione asercje na helpery (`5bee49a`), bez dotknięcia produkcji lub kryteriów. Pełna bramka
gałęzi przeszła **19/19 w 49,35 s**, a `integrate.sh` zakończył lądowanie `6926cb3` pełną bramką
trunka **16/16 w 154,37 s**. Drzewo jest czyste i `TASK.md` nie przeżył lądowania. T-105 i T-110
pozostają zamknięte; następnym zadaniem jest T-99.

## 2026-08-24, 15:25 — T-110 ZAMKNIĘTE, pełny zakres przejmuje T-111

**T-110 · czerwone / ZAMKNIĘTE · około 1 h 11 min do kontrolowanego przerwania · koszt
widoczny: brak kwoty w paragonie.** Dwie tury Codeksa zużyły łącznie **14,47 mln tokenów
wejścia i 51,4 tys. wyjścia**; artefakty recenzji i naprawy Claude'a nie zapisały kwoty.
Kontrakt uczciwie certyfikował 3 AC, implementacja dostała jedną opinię cross-vendor i dokładnie
jedną rundę naprawczą. Po niej AC-1/AC-2/AC-3 były zielone, a pełna bramka doszła do starego
`lead_evidence_is_durable.rs` i nie skończyła się w swoim suficie: jego atrapa App Servera nie
odpowiadała na nowe `config/read`. Proces testu i jego grupa zostały zatrzymane po rozpoznaniu
dokładnych pgid; ponowna sonda drzewa procesów była pusta. Gałąź nie została wylądowana.

To jest wynik granicy, nie zaproszenie do poprawienia speca. Sam kontrakt wskazał przed
implementacją, że ta pełna fikstura wymaga zmiany, ale pliku nie było w OWNS. Po jednej rundzie
naprawy kryteria zadania przeszły, lecz produktowa suita zawisła dokładnie na tym brakującym
ogniwie. Zgodnie z regułą fazy „wykonalne tylko plikiem spoza OWNS = ZAMKNIĘTE” T-110 nie jest
wznawiane i żaden jego commit nie trafia do main.

Recenzent zgłosił pięć uwag. Utratę zatwierdzonych Connections naprawił jeszcze bieg, ale nie
wylądowała z zamkniętą gałęzią. Dwie uwagi o semantyce nakładki rozstrzygnęły później źródła
OpenAI: `ThreadStartParams.config` jest mapą, `config/read` ma oficjalny parametr i kształt,
a `ConfigManager::load_with_cli_overrides` konwertuje pary żądania do TOML i dokłada je po
nakładkach CLI; `mcp_servers.<id>.enabled=false` jest oficjalnym przełącznikiem. Uwaga o obrazie
nie jest defektem: T-34 świadomie nie pokazuje arbitralnego błędu vendora przy załączniku i ma
na to żywe E2E; pełna treść pozostaje dla wiadomości tekstowej. Rozjazd web-dialu jest osobnym,
niezweryfikowanym znaleziskiem i nie został ukryty w tym zadaniu.

Właścicielski wyjątek authoringu tworzy **T-111** z nowymi, globalnie unikalnymi ścieżkami.
Obejmuje oba stare serwery-atrapy, autorytatywną listę Connections i wspólny encoder klucza,
więc nie powtarza granicy T-110. T-111 jest następnym i ostatnim zastępstwem przed T-99;
T-105 ani T-110 nie wolno wznawiać.

## 2026-08-24, 14:04 — T-105 ZAMKNIĘTE, cel przejmuje T-110

**T-105 · czerwone / ZAMKNIĘTE · 14 min ostatniego przebiegu · $0,00 widoczne.** Kontrakt
Codeksa zużył łącznie 7,29 mln tokenów wejścia i 26,4 tys. wyjścia w fazie specyfikacji oraz
jednej naprawy; księga nie wycenia Codeksa. AC-1 i AC-2 dostały uczciwe czerwone testy, lecz
AC-3 po dwóch wymuszonych `before` nadal miało exit 0 bez dowodu wykonania. Gałąź nie została
wylądowana.

Powód nie jest brakiem pracy agenta. Wymagane `--ignore-user-config` działa w `codex exec`,
ale zainstalowany `codex-cli 0.149.1` odrzuca je przed i po `app-server`; pomoc App Servera
tej flagi nie wystawia. Dodanie asercji na argv zazieleniłoby atrapę i zepsuło prawdziwego
leada — byłoby oszustwem. Również `-c 'mcp_servers={}'` nie jest zastępstwem: pusta tabela
scala się niedestrukcyjnie, więc istniejące serwery pozostają włączone.

Właścicielska zgoda na pełne domknięcie fazy została użyta wyłącznie do authoringu nowego,
globalnie unikalnego `T-110`, bez łatania T-105. T-110 zachowuje dwa działające cele i przed
`thread/start` pobiera efektywną konfigurację przez `config/read`, po czym dla każdego
znalezionego serwera ustawia osobne `mcp_servers.<id>.enabled=false` w konfiguracji wątku.
Błąd odmawia startu; nie ma cichego powrotu do prywatnych MCP. T-110 musi wylądować przed
T-102 i jest następnym biegiem.

## 2026-08-24, 13:36 — faza 7: T-98 w trunku, pierwszy żywy bieg zmienił mapę

**T-98 · zielone · 1 h 52 min 26 s · $28,58 widoczne.** Przelotka obu vendorów nie może już
nadpisać transportu, polityki, połączeń, modelu ani limitu wydatku ustawianych przez Loadout;
podniesienia uprawnień są rozpoznawane po kluczu i wartości w obu drogach — przy zapisie oraz
przy Starcie. Po ręcznym domknięciu integracji pełna bramka trunka przeszła **16/0 w 57,64 s**,
a `TASK.md` nie przeżył lądowania (`3700831`, poprawka integracyjna `e175860`).

Próg $25 został przekroczony z konkretnych powodów. Pierwszy start wykrył zduplikowaną globalnie
ścieżkę wyroczni AC-4. Drugi ujawnił wyścig dwóch `worktree.sh`: równoległe zapisy uszkodziły
zarówno `~/.codex/config.toml`, jak i tymczasowy plik `~/.claude.json`. Naprawa harnessu używa
teraz jednej blokady, unikalnych plików tymczasowych i atomowej publikacji dla obu konfiguracji
(`465ec3e`, strażnik zarejestrowany w `6e78c7b`). Recenzent zgłosił cztery słuszne uwagi o sile
wyroczni; wszystkie cztery zostały zamknięte testami zachowania. Pełny clippy po merge'u złapał
jeszcze podwójne włączenie starego modułu T-36 — dlatego ostatni dowód był wykonany po ręcznej
poprawce integracyjnej, nie przed nią.

Otwarte znaleziska z T-98, poza jego kontraktem: rodzina `sandbox_workspace_write.*` rezerwuje
dziś tylko `network_access`, a nie np. `writable_roots`; goły klucz `mcp_servers` nie wpada pod
prefiks `mcp_servers.`; zdanie odmowy limitu wydatku mówi o uprawnieniach do plików. Zostają
tu jako fakty do osobnego kontraktu, nie jako ciche rozszerzenie wylądowanego zadania.

### Pierwszy prawdziwy bieg po fazie 6

Bieg `20260824-091300__01a0330b-6690-7eb2-a156-5613c14d0c9d` trwał **97,5 min**, wykonał
28 kroków przy trzech naraz i zakończył 26 sukcesami oraz dwiema porażkami. Widoczny koszt
Claude'a to **$26,86**; 15 kroków Codeksa zużyło 45,2 mln tokenów wejścia i 218 tys. wyjścia,
ale stara księga pokazała dla nich $0. Raport produktu powstał i przeszedł własne testy.

Żywy przebieg potwierdził obietnice fazy 6, których atrapy nie mogły dowieść: runda trzecia
dostała własne wcześniejsze próby i oba werdykty, fan-in dostał sześć przekazań, wszystkie
osiem tur sprawdzających wystawiło `outcome:`, kopie pracowały osobno, a pamięć przeszła pełne
koło produkcja → promocja człowieka → konsumpcja → `last_used_at`.

Jednocześnie dał trzy nowe dowody, włączone do fazy 7 przed następnym biegiem:

1. **N1 → T-109.** Sześć równoległych procesów Claude'a dzieliło `HOME` i zapisywało ten sam
   `~/.claude.json`. Jeden z nich dostał błąd parsowania JSON i padł po 273 ms z kodem 1;
   CLI zrobiło kopię i odbudowało plik, więc późniejsze kroki ruszyły. Ten `processExit` nadał
   całemu biegowi stan `failed`, mimo że nie był porażką pracy agenta. Kroki dostaną prywatny
   katalog stanu bez utraty równoległości; gospodarz nie może być ich wspólnym plikiem zapisu.
2. **N2 → T-99 AC-2.** W 20 z 28 przekazań pełna kopia była w `attachments/`. Limit 8 KB
   systematycznie usuwał końcową linię `outcome:` z pliku czytanego przez syntezę, chociaż
   silnik rozstrzygnął ją z surowej odpowiedzi. Ucięta kopia ma zachować tę jedną linię
   dokładnie raz, niezależnie od jej położenia, obok bezwzględnego adresu pełnej kopii.
3. **N3 → T-99 AC-3.** Martwy krok wszedł do następnych rund jako 434-bajtowe przekazanie
   z trzema pustymi nagłówkami. To żywe potwierdzenie istniejącego kryterium „left nothing",
   nie nowy zakres.

Druga porażka biegu była prawdziwym wynikiem: sprawdzający ostatniej rundy nie przepuścił pracy.
Mechanizm zadziałał, lecz `carry-on` pozwolił iść dalej; naprawę tej klasy prowadzą T-100/T-101.

## 2026-08-24, 01:40 — FAZA 6 ZAMKNIĘTA: dwanaście z dwunastu w trunku

Wszystkie zadania `T-86`…`T-97` wylądowały, bramka trunka zielona po każdym. Plan i mapa
znalezisk: `docs/PLAN-AGENTS-CONTEXT.md`. `ARCHITECTURE.md` uzgodniony z kodem tego samego dnia.

### Liczniki, z realnych danych

| | |
|---|---|
| Zadania | **12 z 12**, 47 kryteriów |
| Commity fazy | 105, w tym 12 lądowań |
| Koszt z transkryptów `ship-task` (6 zadań) | **$168,61** |
| Tryb szybki (6 zadań) | koszt w sesji orchestratora, nieliczony osobno |
| Decyzje oddane człowiekowi | **4** — i wszystkie cztery były prawdziwymi rozwidleniami |
| Rundy naprawcze / restarty | 5 (T-86, T-90 ×2, T-92 ×2, T-94) |
| Konflikty scalania rozwiązane ręcznie | 6 |
| Defekty złapane dopiero **pełną bramką na trunku** | **3** |

### Trzy rzeczy, które ta faza udowodniła o samym harnessie

1. **Brak konfliktu w gicie nie znaczy poprawności.** Trzy razy scalenie dwóch zielonych gałęzi
   dało drzewo, które się nie kompilowało albo nie przechodziło typów: przeniesiony wektor
   pożyczony osiemnaście linii niżej (T-92 × T-94), funkcja bez klamry zamykającej, bo git
   przyciął hunk na sygnaturze (T-90 × T-97), literał linii bez pól, które właśnie doszły do
   drutu (T-94 × T-97). **Jedynym świadkiem był kompilator i pełna bramka po ręcznym scaleniu.**
   Wniosek operacyjny: po każdym ręcznym rozwiązaniu konfliktu `cargo check --all-targets
   --keep-going`, a potem `./verify.sh full` — nigdy sam commit.
2. **`TASK.md` przeżywa ręczne scalenie.** `integrate.sh` kasuje go tylko na własnej ścieżce
   commita. Zostawiony sprawia, że każdy nowy worktree rodzi się z cudzym kontraktem, a
   `ship-task.sh` odmawia startu. Zdejmowany trzy razy w tej fazie.
3. **Zadanie o pięciu kryteriach dotykające czterech warstw nie mieści się w fazie kontraktu.**
   T-94 spaliło 81 tur i $12,06, nie napisawszy ani jednego pliku (`error_max_turns`). To samo
   zadanie w trybie szybkim, gdzie specyfikacja i implementacja dzielą jeden kontekst, przeszło
   za pierwszym podejściem. **Nie jest to wada modelu, tylko kształtu wywołania.**

### Cross-vendor zarobił na siebie trzy razy

Recenzent Codeksa zgłosił łącznie 12 uwag na zielonych kryteriach. Dwie były prawdziwymi
defektami kontraktu (`giveUpAfterMinutes: 0` obiecywane jako brak limitu przy silniku robiącym
`.max(1)`; AC-3 z T-90 rzekomo sprawdzające odmowę po turze agenta), sześć dotyczyło **siły
wyroczni**, a cztery obaliłem czytając kod. **Ani jednej nie przyjąłem na słowo** — każda
kosztowała 3–5 minut sprawdzenia i to jest właściwa cena.

### Co zostaje otwarte dla człowieka

- **`--settings` nie jest flagą zarezerwowaną**, a od T-92 Loadout ustawia ją sam.
  `agents_vendor_args_filtered.rs` używa jej wprost jako przykładu flagi **nie**zarezerwowanej,
  więc dopisanie jej do listy zmienia przesłankę tamtego testu — **decyzja, nie poprawka.**
- **Trzy długi z T-94, wszystkie po jednej linii w cudzym pliku:** kolizja przelotki
  z `--max-budget-usd` (jedna pozycja w `FORBIDDEN_ESCALATIONS`); pasek `$3.41 of $20` jest
  **liczony i nigdy nie pokazany**, bo `index.tsx` woła `stripFor` bez trzeciego argumentu;
  szew sterownika na flagę budżetu obok `effort_argv`.
- **Chip `12k tokens` jest niebudowalny**: słowo „tokens" jest zakazane przez sprawdzacz
  słownictwa, a `checks/` jest poza zasięgiem biegu. Pisarz T-97 cofnął zmianę zamiast walczyć
  z bramką — słusznie.
- **Kryterium, które liczy elementy, dryfuje po cichu.** `agent-form.test.tsx` miał stałą
  `THREE` z czterema pozycjami. Nota w `tasks/T-11.md`.
- **Pamięć per projekt** (`<repo>/.loadout/memory/`) dalej nie istnieje — zostaje globalnie,
  zgodnie z domyślną decyzją z planu §6.

### Czego ta faza NIE dowiodła

Ani jedno kryterium nie uruchomiło prawdziwego biegu z prawdziwymi agentami. Wszystkie dowody
stoją na `FakeDriver`, złotych plikach i `renderToStaticMarkup`. **Pierwszy prawdziwy bieg
workflow po tej fazie jest testem, którego bramka nie umie zrobić** — i to jest najbliższa
rzecz do zrobienia, zanim dołoży się cokolwiek nowego.

## 2026-08-24, 00:40 — faza 6: dziewięć zadań w trunku, tryb szybki się sprawdził

Kolejność lądowań po pierwszym wpisie: T-91, T-96, T-95, T-88, T-93, T-92. Trunk zielony po
każdym (`integrate.sh`, pełna bramka 15/0). Zostały T-90, T-94, T-97.

**Co realnie dostał produkt.** Pętla pamięta swoje rundy i oddaje dalej to, co przeszło (T-87).
Wznowienie niesie przekazania poprzedniego biegu razem z załącznikami (T-88). Poziom myślenia
dociera wreszcie do obu vendorów — `--effort` u Claude'a, `-c model_reasoning_effort` u Codeksa
(T-91). Katalog roboczy znika po biegu, praca zostaje na gałęzi, a historia umie te gałęzie zdjąć
(T-95). Krok pożycza z repo gospodarza to, co człowiek zaznaczył, i wybór jest własnością kafelka
(T-93). Ekran agenta przestał kłamać o tym, co dostał, a powtórzenie kroku ma wreszcie drogę
na ekran (T-96). **Pamięć ma producenta** — jedna tura refleksji po biegu, najwyżej trzy
kandydatki, każda z uzasadnieniem; auto-pamięć Claude'a pisze do katalogu biegu zamiast do
wspólnego katalogu projektu (T-92).

### Tryb szybki: co zdjęte, co zostaje

Właściciel zdjął pętlę zadaniową dla prostych zadań. Zdjęte: faza kontraktu jako osobne płatne
wywołanie, druga opinia i runda naprawcza. **Zostaje dowód** — własny worktree, kontrakt
zamrożony jako pierwszy commit gałęzi, `./verify.sh before` czerwony NA ASERCJI przed
implementacją, pełna bramka przed lądowaniem i druga pełna bramka na trunku po merge'u.
Tak wylądowały T-91, T-96, T-95 i T-93; recenzenta zostawiono przy zadaniach ruszających silnik.

### Trzy rzeczy, które ta faza kosztowała i których nie było w planie

1. **Trunk był czerwony w warstwie `full` od `905ef9e`** i przewrócił oba zadania pierwszej fali,
   zanim ktokolwiek napisał linijkę. `quick-clippy` jest `--lib`, `full-clippy` `--all-targets`,
   więc quick świecił 13/0. Naprawione `4fcab5c`.
2. **Nowe pole w strukturze przewraca każdy jej literał** — trzy razy z rzędu zadanie stanęło
   na tym samym. Zmierzone: `AgentStep` ma pięć literałów, `RunRequest` **55** i nie ma `Default`,
   `Line::Done` pięć (wszystkie w kryteriach T-05, T-07, T-10). Od T-94 liczę to **gerpem przed
   odpaleniem biegu**, nie po czerwonej bramce.
3. **`commands.golden.txt` musi być w OWNS każdego zadania dokładającego komendę** — trzy
   przeoczenia tego samego kształtu (T-93, T-95, T-92). Rejestracja bez wiersza to czerwień,
   wiersz bez rejestracji to martwa kontrolka.

### Cztery decyzje właściciela, wszystkie z dowodem mechanicznym

Każde poszerzenie zakresu szło z porównaniem linii `## AC-`, `check:` i `expect:` przed i po —
za każdym razem 0 różnic. Rozstrzygnięcia: asercja równości promptu zamieniona na „zawiera raz,
na początku" (T-86); zero minut znaczy w silniku brak limitu, nie jedną minutę (T-86, znalazł
recenzent Codeksa na zielonych kryteriach); obserwacja w cudzym instrumencie przeniesiona tam,
gdzie fakt nadal jest — treść z gałęzi, rejestracja drzewa w trakcie kroku (T-95, kryterium T-65);
refleksja jedzie własnym szwem z domyślnym `None`, nie fabryką sterowników (T-92).

**Ta ostatnia była jedyną zmianą TREŚCI kryterium w całej fazie** i warto wiedzieć dlaczego:
moje AC-1 kazało wołać fabrykę sterowników, czyli tę samą, którą podstawiają wszystkie testy —
28 testów w 20 plikach zobaczyło jedno wywołanie więcej. Tych liczb nie wolno było podnieść:
pilnują, żeby bieg nie odpalił więcej procesów, niż miał.

### Znaleziska, które zostają otwarte

- **Kryterium, które liczy elementy, dryfuje po cichu.** `agent-form.test.tsx` (kryterium T-11)
  ma stałą nazwaną `THREE` trzymającą **cztery** pozycje, a tekst kryterium mówi „dokładnie trzy".
  Ktoś dołożył wiersz i nie tknął ani nazwy, ani tekstu. Nota dopisana do `tasks/T-11.md`.
- **`--settings` nie jest flagą zarezerwowaną**, a od T-92 Loadout ustawia ją sam (przekierowanie
  auto-pamięci). `agents_vendor_args_filtered.rs` używa jej wprost jako przykładu flagi
  **niezarezerwowanej**, więc dopisanie jej do listy zmienia przesłankę tamtego testu —
  **to jest decyzja, nie poprawka.**
- **T-94 spaliło 81 tur i $12,06 w fazie kontraktu, nie pisząc ani jednego pliku** (`error_max_turns`).
  Zadanie o pięciu kryteriach dotykające `AppState`, `limits`, argv i frontu nie mieści się
  w budżecie tur jednej fazy kontraktowej. Przeniesione do trybu szybkiego, gdzie specyfikacja
  i implementacja dzielą jeden kontekst.
- Trzy uwagi recenzenta o **sile wyroczni**, nie o kodzie: AC-5 z T-87 nie przechodzi gałęzią
  „zapytaj mnie"; AC-1 z T-92 nie sprawdza limitu czasu refleksji (sam limit i zdejmowanie grupy
  procesów są w kodzie i sprawdziłem je); AC-2 z T-92 nie sprawdza licznika odrzuconych par.

## 2026-08-23, 20:45 — faza 6 ruszyła: T-89 w main, T-86 stoi na kolizji, trunk był czerwony od rana

Plan fazy i mapa 38 znalezisk: [`docs/PLAN-AGENTS-CONTEXT.md`](PLAN-AGENTS-CONTEXT.md).
Dwanaście kontraktów T-86…T-97 wylądowało jako `7206f4b` (wyjątek właściciela, AGENTS.md §2).
Fala 1 = T-86 + T-89 równolegle, cross-vendor (`--reviewer codex`, kredyty wróciły).

**Trunk był czerwony w warstwie `full` od `905ef9e`, i nikt tego nie widział.** `quick-clippy`
biegnie `--lib`, a `--all-targets` jest dopiero w `full-clippy`, więc trunk pokazywał 13/0 na
quick i przewracał każde zadanie, które doszło do pełnej bramki. Zmierzone: **oba** zadania fali 1
dostały tę samą czerwień w `runs_left_over_are_reconciled.rs:387` — pliku, którego żadne z nich nie
posiada (`git log <baza>..<gałąź> -- <plik>` pusty dla obu). Kosztowało to dwie rundy recenzji
i dwie rundy naprawcze. Naprawione osobnym commitem `4fcab5c` (stała przed instrukcjami);
`clippy --all-targets clean over 68 Rust files`. **Wniosek dla pętli: commit wchodzący na main
poza `integrate.sh` nie przechodzi warstwy `full` i trunk może być czerwony przy zielonym quick.**

**T-89 w main** (`integrate.sh`, bramka 15/0 w 45,8 s). Kafelek „sprawdź" da się wreszcie postawić
z płótna: przycisk, własny panel (komenda, wzorzec przejścia, folder, co po porażce), czerwona
kropka przy braku wzorca, plus dowód po prawdziwym kliknięciu w `e2e/`. Do dziś ten rodzaj kroku
istniał wyłącznie w Ruście i przychodził tylko z importu — czyli jedyny węzeł, który mówi
**co się stało** zamiast **co agent powiedział** (D6, `00-SYNTHESIS` §2.1), nie miał jak trafić
na płótno.

**T-86 w main** (`integrate.sh`, bramka 15/0 w 154 s). Stanął był na dwóch sprawach, obie
rozstrzygnął właściciel tego samego wieczoru; opis obu niżej, bo mechanizm powtórzy się w tej
fazie jeszcze nieraz. Zapłacone: jedna runda kontraktu i dwie implementacje, bo pierwszy bieg
skończył się kodem 1 na czerwieni spoza własnego zakresu.

**Wznowienie było certyfikowane, nie darmowe.** `ship-task.sh` sam orzekł, że kryteria
przechodzą, a specyfikacje niosą komplet asercji z chwili, w której bramka udowodniła je
czerwonymi — czyli „to działająca implementacja, nie rozluźniony kontrakt" — i przeszedł prosto
do fazy implementacji. Drugi kontrakt nie został napisany ani opłacony.

### T-86, sprawa pierwsza: asercja równości kontra nowy blok

`product_path_end_to_end.rs:164` żąda, żeby prompt kroku był **równy** instrukcji, słowo w słowo:

```rust
assert_eq!(prompts, vec![WHAT_TO_DO.to_owned()],
    "the step's instructions have to reach the driver, once, word for word. …");
```

T-86 AC-1 żąda, żeby prompt **każdego** kroku agenta kończył się blokiem mówiącym, że ostatnia
wypowiedź jest tym, co krok przekazuje dalej. Oba zdania nie mogą być prawdziwe naraz.

Co jest ważne przy tej decyzji, i co sprawdziłem, zamiast zgadywać:

1. **Żadne kryterium nie woła tego pliku.** `grep "check:" tasks/*.md` nie wymienia go ani razu —
   to test regresyjny żyjący w scalonym celu `it`, sądzony wyłącznie przez `full-test`.
   Kolizja jest więc między **kryterium T-86** a **asercją bez kryterium**, nie między dwoma
   kryteriami.
2. **Zdanie, które ta asercja niesie, jest po T-86 nadal prawdziwe.** Instrukcja człowieka
   dociera do sterownika dosłownie i dokładnie raz — stoi na początku promptu, przed blokiem.
   Nieprawdziwa robi się wyłącznie **forma** asercji (równość całego promptu), nie jej treść.
3. **Defekt, który ta asercja złapała, zostaje złapany po każdej możliwej zmianie:** pusty
   `instructions` dalej daje prompt bez zdania człowieka.

Trzy wyjścia, w kolejności, którą rekomenduję:

- **(a)** Zamienić równość na „zawiera dosłownie, dokładnie raz, na początku" — zdanie asercji
  zostaje bez zmiany, defekt pustego promptu dalej czerwony. Wymaga dopisania
  `src-tauri/tests/it/product_path_end_to_end.rs` do OWNS T-86 z **wąskim mandatem** (skill §5c
  pozwala poszerzyć uprawnienia, nigdy kryteria; porównanie linii `## AC-`/`check:`/`expect:`
  przed i po jest wtedy obowiązkowe).
- **(b)** Zostawić asercję i zwęzić AC-1 do „krok, który ma następnika" — słabsze, bo krok
  końcowy też oddaje przekazanie, a to on najczęściej niesie wynik całego biegu.
- **(c)** Uznać, że blok nie wchodzi do promptu, tylko do `--append-system-prompt` — nie działa
  u Codeksa, który nie ma takiej flagi i dostaje system prompt doklejony do stdin.

Nie wybrałem sam, bo (a) rozluźnia asercję, którą napisano po prawdziwym incydencie, a §5 karty
orchestratora zabrania mi rozluźniać oracle, żeby przepuścić własną falę.

**Rozstrzygnięte: (a).** Poszerzenie zakresu, nigdy kryteriów, z dowodem mechanicznym w commicie
`6398ea5`: `diff` linii `## AC-`, `check:` i `expect:` między kontraktem certyfikowanym na gałęzi
a nowym dał **0 różnic** przy 9 liniach kryterialnych po obu stronach. W kontrakcie stoi wąski
mandat i **wypisane wprost obejście, którego nie wolno zrobić** — gołe `contains()` bez
`starts_with` i bez liczby wystąpień. Pisarz wykonał mandat co do joty: trzy warunki naraz
(`len() == 1`, `starts_with`, `matches().count() == 1`), zdanie asercji nietknięte słowo w słowo,
plus komentarz nazywający ten sam defekt, po którym asercję napisano.

### T-86, sprawa druga: `giveUpAfterMinutes: 0` nie znaczy „bez limitu"

Znalazł to **recenzent Codeksa** (`gpt-5.6-sol`), na zielonych kryteriach — dokładnie ten
mechanizm, dla którego D3 wymaga cross-vendora:

> AC-2's assertion accepts a false promise: `giveUpAfterMinutes: 0` is described to the agent as
> having no time limit, but the execution timer converts it to one minute with `.max(1)`.

Ma rację i to jest **defekt kontraktu, który napisałem**, nie implementacji. `plan_agent` liczy
`give_up_after_minutes.max(1) * 60`, więc `0` to dziś **jedna minuta**, a nie brak limitu.
Prompt mówiący „nie masz limitu" przy kroku ubijanym po 60 s jest gorszy niż brak zdania.

Dwa wyjścia: albo silnik zaczyna traktować `0` jako brak limitu (`run.rs` **jest** w OWNS T-86,
więc mieści się w zakresie, ale to zmiana zachowania poza literą kryterium), albo AC-2 przestaje
obiecywać brak limitu i prompt mówi prawdę o jednej minucie. Pierwsze jest lepsze dla produktu
(pole „0" w formularzu agenta oznacza dla człowieka „bez limitu"), drugie mieści się w kontrakcie
bez jego zmiany.

**Rozstrzygnięte: silnik.** `plan_agent` daje dziś `0 => Duration::MAX`, a liczba minut jedzie do
promptu **osobnym polem**, nie wyjęta z `Duration` — pisarz zauważył sam, że zdanie zbudowane
z `Duration::MAX` obiecywałoby agentowi pięćset osiemdziesiąt cztery tysiące lat. AC-2 nie
zmieniło się ani o słowo; zmieniło się to, czy jego zdanie jest prawdziwe.

**Recenzent Codeksa zgłosił przy drugim biegu dwie dalsze uwagi i obie są o SILE WYROCZNI, nie
o kodzie** — zapisuję je, bo są prawdziwe i nikt ich dziś nie egzekwuje: AC-2 dowodzi wyłącznie,
że krok bez limitu przeżywa jedną wirtualną godzinę, więc implementacja zamieniająca zero na
dowolny skończony limit powyżej godziny też by przeszła (zbudowana jest `Duration::MAX` —
sprawdzone w kodzie, nie w teście); a AC-1 czyta blok jako „wszystko od pierwszego znacznika do
końca promptu", więc nie wykryłaby tekstu doklejonego ZA blokiem.

## 2026-08-22, 18:20 — T-79 w main: skille docierają do vendora, potwierdzone przez vendora

`131d214`. Bramka gałęzi 20/0, bramka trunka po lądowaniu 15/0 w 110 s.

Zbiór efektywny liczy się z agenta złożonego z nadpisaniem kroku; brak klucza znaczy „weź to,
co ma agent", `[]` znaczy żadnych, lista znaczy podzbiór skilli tego agenta. Nazwa spoza
zbioru zatrzymuje bieg **przed pierwszym procesem**, z nazwą brakującego skilla w zdaniu.
`RunSpec` nietknięty zgodnie z rozstrzygnięciem właściciela — wybór jedzie istniejącym szwem
dziedziczenia.

**Najmocniejszy dowód w tym biegu**: AC-3 uruchamia PRAWDZIWE Claude Code z tym samym
fragmentem argv, bierze linię `system`/`init` z transkryptu i przepuszcza ją przez
`place::discovery_from_init` — `Seen` dla obu wybranych, `NotSeen` dla trzeciego. Odpalone na
żywym CLI: 3,75 s, vendor ogłasza `<plugin>:alpha` i `<plugin>:beta`. To nie jest „napisaliśmy
pliki do katalogu"; to vendor mówi, że je widzi.

Cztery biegi zamiast jednego. Pierwszy padł, bo faza kontraktu napisała 33 KB specyfikacji bez
ani jednego `mod` w `tests/it/main.rs`; drugi i trzeci dowiozły resztę; czwarty przeszedł po
naprawie `before-spec-owns`. Obie przyczyny opisane niżej.

### Dwie rzeczy, które T-79 zostawia człowiekowi

1. **AC-3 jest naprawione w połowie.** Wyrocznia sięga do konta i sieci, więc musi być
   `#[ignore]`, a linia `check:` tego kryterium nie ma `--include-ignored`. Wzór: T-04 AC-6.
   Do czasu dopisania bramka dowodzi z dysku i manifestu, a dowód od vendora przechodzi się
   ręcznie: `cargo test --test it skills_reach_claude:: -- --ignored`. Cena dopisania jest
   realna: każdy bieg bramki zaczyna kosztować wywołanie vendora i wymagać sieci.

2. **AC-5 dowodzi, że callback działa, gdy się go zawoła — nie że Start go woła.** Pisarz
   odmówił naprawy przez skrót i nazwał powody: `go()` czyta `choices` wypełniane przez
   `useEffect`, którego `renderToStaticMarkup` nie uruchamia; DOM-u nie ma, bo vitest biegnie
   w `node`, a jsdom, happy-dom, `@testing-library` i `react-test-renderer` nie leżą
   w `node_modules`; Playwright odpada, bo `e2e/` jest poza OWNS tego zadania. Wybór: devDependency
   na środowisko DOM plus zmiana linii `check:`, albo przeniesienie kryterium do `e2e/`.

## Szósty defekt harnessu: `before-spec-owns` nie umiał rozwiązać celu `it`

`e9ddaae`. `CARGO_TARGET` w tym pliku był jednogrupowy, więc `--test it <modul>::`
rozwiązywało się na `src-tauri/tests/it.rs` — plik, który nie istnieje. `harness/gate.py:326`
ma na tę samą składnię regex dwugrupowy. Piąty konsument tej składni czytał ją inaczej niż
cztery pozostałe.

Skutek: składni używa **56 plików zadań**. Przy zadaniu czysto rustowym check wypadał przez
furtkę „the specs do not exist yet" z kodem 0 — milczał tam, gdzie miał sądzić. Przy mieszanym
sądził sam front i oskarżał kontrakt na pusto.

Kontrola pozytywna po naprawie: przegląd wszystkich kontraktów — 81 sądzonych i zielonych,
6 milczących, **0 czerwonych**. Kontrola negatywna: OWNS na `engine/limits.rs` z kryterium
w `store_pragmas::` daje 1 i wypisuje poprawnie rozwiązaną ścieżkę.

**Znalezisko przy okazji, nienaprawione:** rozróżnianie tego checku jest słabe. Pierwsza wersja
kontroli negatywnej PRZESZŁA, bo spec magazynu trafił w symbol `Result` z plików OWNS. Filtr
odrzuca nazwy do trzech znaków, więc zwykłe angielskie słowa przeciekają. Zaostrzenie wymaga
pomiaru na 81 kontraktach — **czeka na człowieka**.

## T-77 stoi na jednej decyzji projektowej

Oba własne kryteria zielone: Import JEST siódmą sekcją, otwiera się, Agenci przestali być drogą
do niego. Padło `shell-matches-mockup`: powłoka ma siedem przełączników, `docs/mockup/index.html`
ma sześć, a makieta jest wyrocznią nawigacji („a different set here is a different product,
not a different style"). **Czeka na człowieka: czy makieta dostaje siódmą pozycję „Import".**
Gałąź `task-T-77` gotowa, dwanaście plików, nic poza OWNS — pisarz uderzył w ścianę i stanął,
zamiast sięgnąć poza zakres.

Mój błąd w autorstwie kontraktu: naliczyłem pięć plików kodujących listę sekcji, bo znalazłem
je gerpem po nazwach. Szósty, `shell-matches-mockup.test.tsx`, nie wymienia ich wcale —
wyprowadza je z makiety.

## 2026-08-22, 15:13 — T-75 w main, T-76 cofnięte pomiarem, cztery defekty harnessu, osiem nowych kontraktów

Właściciel polecił zacommitować zastaną pracę, wyładować gałęzie importu i zacząć budowę
domknięcia importu setupu. Wykonane wszystko poza wyładowaniem T-76, które **cofnięte**.

**Zastana praca T-34 zacommitowana bez pętli.** 62 pliki (+7939/-793) leżały na main
niezacommitowane: dowody biegu, allowlistowany raport diagnostyczny i obrazy wklejane do
rozmowy Lead. Powstały bezpośrednio na trunku — bez worktree, bez czerwonego `before`, bez
drugiej opinii. Nie da się tego odtworzyć wstecz, więc jest to zapisane w commicie `800ebc3`
zamiast udawać zwykłą drogę. Jedyny dowód jest zewnętrzny wobec kryteriów T-34: pełna bramka
zielona. **Nikt nie sprawdził, że sześć kryteriów T-34 jest czerwonych bez implementacji** —
czyli nie wiadomo, czy cokolwiek mierzą. To zostaje otwarte.

**T-75 wylądowane** (`9564616`). Cztery konflikty, wszystkie tego samego kształtu: T-34 i T-75
dokładają do tych samych typów dwa równoległe, dyn-safe szwy z domyślnym `None` —
`with_evidence` i `configured`. Rozwiązane sumą. Jedno miejsce wymagało decyzji: w
`commands/run.rs` oba opakowania oddają KLON sterownika, więc kolejność (Connections →
dziedziczenie → dowody) jest wymuszona i milcząca; odwrócenie kompiluje się i cicho gubi
`--mcp-config` albo plik dowodu. Powód stoi w komentarzu przy tych liniach.

**Dwie wady, których git nie zgłosił jako konflikt.** `lib.rs:299` — automatyczny merge skleił
dwa ogony jednego komentarza blokowego i plik przestał się parsować, z meldunkiem
„Auto-merging". Trzy literały struktur w testach `codex.rs` bez nowego pola (E0063). Obie
znalazł dopiero `cargo check --all-targets --keep-going`; bez `--keep-going` druga wyszłaby
po naprawie pierwszej.

**T-76 wylądowane i cofnięte** (`bdc622b`, revert `7e77548`). Bramka po merge'u czerwona:
`full-test` 15/1, dwa testy z `setup-is-real.test.tsx`. Przyczyna to kolizja kontraktów, nie
wada merge'a: T-75 AC-10 obiecuje „człowiek uruchamia Scan, widzi cztery statusy, wszystkie
blockery", a T-76 zamknął całą tabelę za `preview.analysis === undefined ? null :`. Kryterium
T-75 uruchomione NA GAŁĘZI T-76 daje `2 failed | 2 passed` — regresja przyjechała z gałęzią,
a bramka gałęzi jej nie zobaczyła, bo tam biegł tylko `verify.sh quick`.

>>> T-76 WYMAGA RUNDY NAPRAWCZEJ: tabela ma być widoczna po Scan, a sekcja analizy ma się do
niej DOKŁADAĆ. I uwaga przy ponownym lądowaniu: git uznaje T-76 za wmergowany, więc samo
`./integrate.sh T-76` wciągnie wyłącznie commity po reverke i cicho cofnie resztę. Najpierw
`git revert 7e77548`, dopiero potem merge. <<<

## Cztery defekty harnessu znalezione po drodze

1. **`quick-permissions` wychodziło 2 na czystym main** — `T-75 owns AGENTS.md, but
   Edit(AGENTS.md) forbids it`. Deklaracja była martwa (zero plików przez dwanaście commitów
   gałęzi), zdjęta w `a8818ce`. `integrate.sh` ma jawną obronę przed lądowaniem na kodzie 2,
   więc T-75 i tak by nie weszło — z komunikatem wyglądającym na winę gałęzi.

2. **Strażnik N-08 był czerwony od 2026-08-16** (`abe8f02`). Wołał
   `refresh_harness_from_trunk` bez `ID`, a ta funkcja mrozi `tasks/$ID.md` — przy pustym `ID`
   mroziła `tasks/.md`, czyli nic, i to cicho, bo `git diff --quiet` na nieistniejącej ścieżce
   jest prawdą. Zmierzone na wyekstrahowanej funkcji: bez ID `contract v2`, z ID `contract v1`,
   oracle `new oracle` w obu. Mechanizm produkcyjny był sprawny; nieaktualne było wywołanie.
   Skąd: `caf976c` zawęził zamrożenie do własnego pliku zadania i tknął wyłącznie ship-task.sh.

3. **Strażniki biegną wyłącznie w `scripts/ci.sh`, a `integrate.sh` woła `verify.sh`.** Bramka
   gałęzi i bramka lądowania ich nie znają, więc każde lądowanie przechodziło ponad czerwienią,
   której żadna z nich nie widzi. To jest decyzja o tym, gdzie mają mieszkać strażniki —
   **czeka na człowieka**.

4. **`quick-scope` ma strażnika, który pudłuje, i cztery sprawdzenia nie mają go wcale.**
   Po naprawie N-08 bramka doszła wreszcie do etapu guards: 10 strzeliło poprawnie, 1 spudłował,
   4 bez strażnika (`before-spec-owns`, `quick-invoke-args`, `quick-tests-listed`,
   `quick-wired`). Pudło: po zdjęciu zasadzonego naruszenia `quick-scope` nadal świeci przez
   `.claude/settings.local.json` i `.claude/worktrees/` — nieśledzone, sprzed tej sesji.
   **Łatwa naprawa jest oszustwem i nie została wykonana**: dopisanie ich do `GENERATED`
   oślepia sprawdzenie na plik, który NADAJE UPRAWNIENIA (`allow: Bash(ps -eo pid,command)`),
   czyli osiąga od drugiej strony dokładnie to, przed czym broni wyłączony `.gitignore`.
   Właściwa naprawa: strażnik ma dowodzić, że sprawdzenie REAGUJE na zasadzone naruszenie,
   a nie że jest zielone w tym środowisku. To zmiana w `harness/guards.sh` dotykająca
   wszystkich jedenastu strażników — **czeka na człowieka**.

Piąte, drobniejsze: `integrate.sh` umie rozwiązać konflikt TREŚCI `TASK.md`, ale nie
SKASOWANIA — a skasowanie robi sam, trzydzieści linii niżej. Każde lądowanie po tym, w którym
TASK.md zniknął z trunka, trafi w `error: path 'TASK.md' does not have our version`.

## Osiem kontraktów na domknięcie importu (`c05bb6b`)

T-77 ekran importu jako sekcja paska · T-78 typowany model i receipt · T-79 skille docierają
do vendora · T-80 pamięć per agent · T-81 MCP: parsery i pętla zwrotna · T-82 rekonstrukcja
workflow · T-83 reimport i naprawa · T-84 tabela Skills.

Trzy rzeczy zmierzone w kodzie, które zmieniły podział wobec planu właściciela:
`connections::runtime` już odmawia startu dla wyłączonego połączenia (dwa kryteria z planu
byłyby zielone w `before`); `RunSpec` nie ma `Default` i konstruuje go 31 plików, więc nowe
pole to fala, a nie linia; siódma sekcja paska kosztuje pięć plików powłoki, z czego trzy są
cudzymi kryteriami.

Stan na teraz: main zielony (15/0, 92,71 s), T-77 biegnie przez `ship-task.sh`.

## 2026-08-21, 13:57 — T-74 w main i uruchomione; Linear ma pełną drogę konfiguracji

Właściciel odrzucił ręczne tworzenie JSON-u po T-65 i polecił zbudować najpierw prawdziwy
connector Lineara. Ekran Triggers prowadzi teraz przez Create/Edit/Delete: wybór Lineara,
jednokierunkowe podanie klucza, prawdziwą listę workflow oraz sprawdzanie co 1, 5, 15 albo
60 minut. Przy cadence ekran mówi wprost, że sprawdzanie działa tylko przy otwartym Loadoucie.
`Test connection` wykonuje osobne zapytanie `viewer`; nie uzbraja triggera i nie zapisuje
kursora, kolejki ani biegu.

**Granica sekretu i zapisu.** Okno nigdy nie dostaje klucza ani jego pochodnej. Rust tworzy
plik jako 0600 przed pierwszym bajtem, publikuje Create bez nadpisania i odmawia stale Edit.
Puste pole edycji zachowuje najnowszy klucz z pliku, wpisany zastępuje go jawnie. Pliki T-65
z `condition: "assigned to me"` i bez cadence nadal się ładują, ale nowe zapisy używają wyłącznie
`assigned-to-me`. Nie twierdzimy, że to Keychain albo szyfrowanie at rest.

**Delete nie ściga się ze Startem.** Pending jest trwale kończone jako Cancelled przed ukryciem
konfiguracji. Bound oznacza, że Start już wiąże bieg, więc Delete odmawia przed jakąkolwiek
mutacją i pokazuje człowiekowi, żeby poczekał; rozpoczęty bieg zatrzymuje Stop. Crashowe pliki
tymczasowe i tombstone mają czytelnika, blokadę per katalog+slug oraz bariery fsync. Niezależny
audyt zakończył się `none` osobno dla frontendu i Rusta.

**Paragon.** Formalne `before` uruchomiło 8 kryteriów i wszystkie były czerwone z właściwego
powodu w 3,60 s. Późniejsze wzmocnienia miały własne celowane czerwienie: między innymi Delete
dla Bound 5/1, symlink korzenia 5/1, współbieżność curl 3/2, publish ledger-temp 7/1 i legacy
condition 0/1. Końcowe kryteria: frontend 31/31, Rust AC-3 10/10 po mechanicznym podziale testu,
AC-4 5/5, AC-7 7/7; sąsiedzi T-65 3/3, 7/7 i 27/27. Pełny rerun miał zielone wszystkie
21 sprawdzeń kodu w 23,79 s, w tym `full-clippy` i `full-test`.

Właściciel jawnie zezwolił usunąć sprzeczny deny dla posiadanego `src-tauri/Cargo.toml`;
`quick-permissions` wróciło do zieleni w `c484b6f`, a T-74 weszło do main jako `81337c2`.
Przed merge'em trunk przeszedł 15/0. Po merge'u bramka dwa razy zatrzymała się wyłącznie na
starszym `workspace_global_slots`: w pełnej równoległej suicie zmierzył peak 2 zamiast 3.
Ten sam test przechodzi osobno (1/1), przeszedł w bramce gałęzi i trunka przed merge'em, a pełne
`cargo test -- --nocapture` po merge'u także przeszło; zwykłe `cargo test` odtwarza peak 2.
Test i jego kontrakt leżą poza OWNS T-74, więc nie zostały cicho osłabione. Merge pozostaje
w main zgodnie z zachowaniem `integrate.sh`, a aplikacja została zbudowana i uruchomiona z tego
SHA. Druga opinia Claude była niedostępna (`api_error`), więc `review.sh` zwrócił 0 jako advisory.
Żywego wywołania Lineara nie wykonano, bo w bramce nie ma klucza; produkcyjny przycisk jest
gotowy do takiego testu.

## 2026-08-21, 09:22 — T-65 gotowe na gałęzi, pełna bramka zielona

Właściciel polecił zaplanować i wykonać T-65 oraz oddał wybór rozwiązania agentowi. Powstał
trwały ledger dostaw pod `~/.loadout/triggers/`, UUID v7 przydzielony przed Startem i pierwszy
atomowy oraz zsynchronizowany `run.json` jako chwila akceptacji. Rozwiązanie nie ufa
`RunState.workflow`: o zajętości decyduje `AppState.live`, a wyścig z ręcznym Startem zostawia
ten sam pending i UUID do ponowienia. SQLite pozostaje indeksem; nadal nie ma daemona, wielu
żywych biegów ani `stop_run(id)` z Etapu B T-71.

**Paragon kontraktu.** Pierwsze `before`: 9/9 kryteriów czerwonych z właściwego powodu w 6,68 s;
po wzmocnieniu recovery AC-8 osobny `before` dał 2/2 w 4,17 s. Końcowy `quick` po aktualizacji
makiety: 21/0 w 9,64 s. Pełna bramka nie została uznana po dwóch częściowych przebiegach: pierwszy
znalazł lint w teście i utratę komunikatu T-64, drugi stare pięciosekcyjne lustro makiety. Po
naprawach oraz jawnej zgodzie właściciela na dodanie `docs/mockup/index.html` do OWNS finalny
`full` dał 23/0 w 29,81 s.

Niezależne audyty przed bramką znalazły i dostały regresje między innymi dla: wszystkich nowych
spraw zamiast tylko najnowszej, local Pending przy niedostępnym Linearze, crashy przed
`run.json`, osieroconej administracji worktree, symlinków w artefaktach biegu oraz ponownego
`fsync` pliku i katalogu przed recovery-acceptance. AC-8 kończy z 27 testami. Druga opinia Claude
była niedostępna (`api_error`); `review.sh` zgodnie z kontraktem zwrócił 0 jako advisory, więc nie
powstał plan rundy naprawczej. Pozostało lądowanie i pełna bramka na trunku.

## 2026-08-21, 01:53 — trzy urwane sesje rozliczone: T-71 i T-64 w trunku, T-40 wycofane, T-65 uczciwie wstrzymane

**Wyladowane: T-71 i T-64.** T-71 przeszlo 20/0 na galezi i 15/0 na trunku; po znalezieniu
urwanej uwagi recenzenta jego AC-4 zostalo w tej samej rundzie wzmocnione o drugie klikniecie
`+` przy juz otwartym terminalu, potem ponownie 20/0 na galezi i 15/0 po ladowaniu. To odroznia
„0 kart → 1" od prawdziwej obietnicy wlasciciela: kolejne terminale w tym samym zakresie nie
podmieniaja poprzednich.

W T-64 wszystkie szesc kryteriow bylo w `before` czerwonych z wlasciwego powodu; potem `quick`
dal 19/0, a `full` 21/0. Klucz Lineara jedzie w konfiguracji `curl --config -` na stdin,
srodowisko jest wyczyszczone do `PATH`, odpowiedz GraphQL jest deserializowana permisywnie wobec
obcych pol, a kursor pod `~/.loadout/triggers/` jest zapisany atomowo przed oddaniem trafienia.
Awaria zapisu jest odmowa, nie trafieniem. Druga opinia Claude byla niedostepna (`api_error`),
co zgodnie z kontraktem review jest notatka advisory i `exit 0`, nie blokada. Po nalozeniu na
T-71 pelna bramka zostala powtorzona (21/0), a trunk po ladowaniu dal 15/0.

**T-40 wycofane pomiarem.** AC-1 przeszlo mimo dwoch celowych martwych handlerow zasadzonych
w produkcji, a AC-2 nie skonczylo sie w 40,42 s. Pierwsze nie widzi obiecanego naruszenia,
drugie nie uruchamia sadu; oba sa zakazanymi pseudo-czerwieniami z AGENTS.md §2a. Galaz
`task-T-40` zostaje jako paragon i nie jest integrowana — niesie mutacje kontrolne, nie naprawy.

**T-65 wstrzymane przed `before`.** `RunState.workflow` klamie po odmowie drugiego startu, ale
zastapienie go samym `ALREADY_GOING` zostawia wyscig: T-64 przesuwa kursor przed Startem, wiec
odmowa po trafieniu zjada sprawe. Przesuniecie kursora po Starcie dubluje ja po awarii miedzy
akceptacja a zapisem. Rozstrzygniecie: potrzebna jest trwala tozsamosc i chwila akceptacji biegu
po stronie Rusta, z ktora da sie zwiazac trafienie i odtworzyc decyzje po restarcie. Etap B jest
nazwany w T-71, ale nie ma pliku zadania; T-65 nie obchodzi tej luki stanem okna. AC-2 i AC-6
zostaly juz poprawione pod niezmiennik 29: przyszly sad ma renderowac prawdziwy ekran.

**Brudny trunk zachowany, nie przepchniety.** Dziewiec plikow z urwanych sesji lezy w commicie
`1fdbefd` na `rescue/2026-08-21-three-sessions`. Czesc rozmowy byla starsza i wezsza od T-71
(watek per zakres zamiast per terminal), wiec zostala zastapiona przez wyladowane T-71. Cztery
pliki wlaczajace `CodexDriver` w aplikacji sa sensowne, ale nie naleza do OWNS T-10 ani zadnego
innego istniejacego zadania; pozostaja zachowane, nie w trunku.

**Trzy luki runtime z pierwszej sesji pozostaja znaleziskami, nie cichymi poprawkami:** produkcja
nie wola `ClaudeDriver::with_transcript`, `copies > 1` nadal nie rozwija krokow, a limit czasu nie
jest widoczny agentowi i przy ubiciu gubi `cost_usd`/`summary`. Zadnego odpowiadajacego pliku
`tasks/<ID>.md` nie ma. AGENTS.md §2 zabrania wymyslenia zadan w przelocie, a §7 zabrania wejscia
w szwy przypisane innym taskom. Handoff zalacznika, ktory ujawnil te luki, jest juz w trunku
(`693f894`, poprawka full-clippy `209ba7f`).

**Dowod, ktorego T-64 swiadomie nie ma:** nie wykonano zywego zapytania do Lineara, bo w repo i
w bramce nie ma klucza. Do sprawdzenia reka po skonfigurowaniu pierwszego triggera: czy zapytanie
GraphQL jest przyjmowane przez aktualne API. Drugi jawny dlug T-64: budowniczy bezpiecznego `curl`
jest teraz drugi obok `skills::ingest`; wspolny rdzen wymaga osobnego zadania z OWNS obu stron.

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
