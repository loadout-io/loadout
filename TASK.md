# T-114 — Kopie, przekazania i sędzia bez kolizji ani fałszywego pochodzenia

T-99, T-112 i T-113 zostały **ZAMKNIĘTE bez lądowania**. Ich gałęzie są dowodem
incydentów, nie źródłem commitów, testów ani implementacji: T-114 startuje wyłącznie z
`main`. Harness ma już osobną naprawę `5604c3d`, więc każdy nowy spec musi się skompilować,
uruchomić i paść na asercji przed implementacją.

T-113 dowiozło pięć zachowań, ale nie może wylądować. Jego spec AC-3 użył jednego helpera
do dwóch różnych faktów i po wznowieniu wymagał etykiety `what the step before left`, choć
przeniesiony plik prawidłowo ma pochodzenie `what an earlier run left here`. Planista i
wykonawca jedynej naprawy odmówili fałszowania pochodzenia albo zmiany zamrożonego speca.
T-114 naprawia kontrakt od początku: pierwsza dostawa i wznowienie mają osobne oczekiwane
etykiety, a oba wiersze wskazują pełną kopię z katalogu swojego biegu.

Pięć rozstrzygnięć właściciela z 2026-08-24 jest kontraktem tego zastępstwa:

1. **Klucz pracy i ref to dwa kodowania jednego faktu.** Druga kopia pracuje w
   `work/s_2~2`, ale jej poprawna gałąź to `loadout/<bieg>/s_2-2`. Pierwsza kopia zachowuje
   dzisiejszą nazwę co do bajta.
2. **Kodowanie nie może scalać dwóch prac w jeden ref.** Jeżeli planowane klucze dwóch
   `fresh-copy`, np. kopia `s_2~2` i literalny krok `s_2-2`, wybrałyby ten sam ogon gałęzi,
   plik dostaje ostrzeżenie przy zapisie, a Start dostaje Problem i odmawia przed katalogiem
   biegu, drzewem roboczym i pierwszym procesem. Hash, losowy sufiks i przemianowanie
   niekolizyjnych gałęzi nie są rozwiązaniem.
3. **Plik przekazania jest przenośny, prompt jest chwilowy, pochodzenie pozostaje prawdziwe.**
   Trwały wiersz `Moved to attachments/<nazwa>__full.md` jest względny. Prompt dopisuje
   bezwzględny adres pełnej kopii w bieżącym biegu. Zwykły następnik widzi
   `what the step before left`; po wznowieniu widzi `what an earlier run left here`.
4. **Końcowa decyzja musi przeżyć cięcie, a pustka musi być widoczna.** Ostatni osobny wiersz
   `outcome:` przeżywa limit dokładnie raz; udany krok z trzema pustymi sekcjami dostaje
   widoczny sygnał `left nothing` w prawdziwym indeksie odbiorcy.
5. **Sędzią pętli jest źródło strzałki powrotnej.** `link.from` zamyka pętlę i wydaje decyzję;
   `link.to` jest jej wejściem. Kilka kopii sędziego jest odmawiane, kilka kopii wejścia
   pozostaje legalne.

Żywy bieg `20260824-091300` pozostaje dowodem dla przekazań: 20 z 28 miało pełną kopię,
cięcie 8 KB usuwało końcowe `outcome:`, a martwy krok wszedł do kolejnych rund jako plik
z trzema pustymi sekcjami bez sygnału w indeksie.

**Read first:** `docs/STATUS.md` (wpisy T-99, T-112 i T-113) · `tasks/T-113.md` wyłącznie
jako opis zamkniętego kontraktu, nie źródło plików · `src-tauri/src/commands/run.rs`
(`work_key_for`, `lay_out_the_run_dir`, `index_of_what_came_before`, `handed_before`,
`seed_the_handoffs`) · `src-tauri/src/commands/isolate.rs` (`branch_for`,
`make_or_recover_git_tree`) · `src-tauri/src/memory/handoff.rs` (`Written::attachment`,
`write_inner`, `cap`) · `src-tauri/src/workflow/check.rs` (`check`, `check_to_run`,
`Link::is_a_way_back`) · istniejące wyrocznie `memory_handoff_cap.rs` i
`resume_carries_the_attachments.rs` — mają pozostać zielone bez zmian.

## Kto to robi

- **Agent:** `rust-core`. Zastępuje T-99/T-112/T-113 i musi wylądować przed T-100.
- **Druga opinia:** Codex w osobnej roli i na innym modelu. Właściciel jawnie wybrał
  Codex + Codex 2026-08-24 ze względu na kończący się budżet Claude'a.

## AC-1 Każda niekolizyjna kopia ma dokładną, poprawną gałąź Gita
check: cargo test --test it t114_copies_get_noncolliding_git_branches::
expect: (\d+) passed

Krok `copies: 2` + `folder: fresh-copy` w prawdziwym tymczasowym repo uruchamia obie kopie
równolegle. Powstają dokładnie gałęzie `loadout/<bieg>/s_2` i
`loadout/<bieg>/s_2-2`, a praca każdej kopii jest osiągalna z właściwej gałęzi. Katalogi pracy
pozostają `work/s_2` i `work/s_2~2`. Kontrole: `copies: 1` zachowuje dzisiejszą nazwę co do
bajta, a wznowienie drugiej kopii odbija się od jej własnej gałęzi, nie od pierwszej.

## AC-2 Kolizja zakodowanych refów odmawia przed pierwszym procesem
check: cargo test --test it t114_colliding_copy_refs_are_refused_before_start::
expect: (\d+) passed

Plik z krokiem `s_2` w dwóch własnych kopiach oraz osobnym krokiem `s_2-2` w `fresh-copy`
jest kolizyjny: `s_2~2` i `s_2-2` wybierają ten sam ogon refa. `check` pokazuje jedno
ostrzeżenie, a `check_to_run` i prawdziwa komenda Start pokazują jedno stałe angielskie zdanie,
które nazywa obie widoczne nazwy kroków i wspólną **work branch**. Start odmawia przed
utworzeniem katalogu biegu, `git worktree`, gałęzi i przed pierwszym wywołaniem sterownika.
Test komendy dowodzi braku tych skutków, nie poprzestaje na wartości helpera. Kontrole:
niekolizyjne kopie pozostają legalne, a literalny krok bez `fresh-copy` nie rezerwuje gałęzi.
Dwa źródła tej samej pary nie dublują zdania.

## AC-3 Czytelnik dostaje adres bieżącej kopii i prawdziwą etykietę pochodzenia
check: cargo test --test it t114_reader_gets_current_attachment_address::
expect: (\d+) passed

Ucięty plik na dysku nadal zawiera dokładnie względny wiersz
`Moved to attachments/<nazwa>__full.md`. W zmontowanym promcie zwykłego następnika jego wiersz
ma dokładną etykietę `what the step before left; full text: <bezwzględna ścieżka>`. Po
wznowieniu przeniesiony wiersz ma dokładną etykietę
`what an earlier run left here; full text: <bezwzględna ścieżka>`. Spec nie może użyć jednego
oczekiwanego labela dla obu przypadków.

Obie ścieżki są regularnymi plikami z bajtami oryginalnej odpowiedzi, ich katalog jedzie w
`extra_dirs`, a adres wznowienia wskazuje kopię pod katalogiem nowego biegu. Skasowanie starego
katalogu nie psuje nowego adresu. Ciało poniżej limitu nie dostaje adresu ani nie tworzy
`attachments/`. Istniejące `memory_handoff_cap.rs` i `resume_carries_the_attachments.rs`
przechodzą bez zmian.

## AC-4 Ostatnia decyzja przeżywa limit dokładnie raz
check: cargo test --test it t114_last_decision_survives_limit::
expect: (\d+) passed

Jeżeli pełne ciało zawiera rozstrzygającą linię `outcome: pass` albo `outcome: fail`, ucięte
ciało zachowuje tę samą, ostatnią rozstrzygającą linię dokładnie raz, nawet gdy w źródle stoi
za limitem 8 KB. Jeżeli zachowana część już ją zawiera, nie powstaje duplikat; brak linii
rozstrzygającej nie produkuje sztucznej decyzji. Pełna kopia pozostaje bajt w bajt oryginałem,
a ciało poniżej limitu nie zmienia się.

## AC-5 Pusta odpowiedź jest nazwana w prawdziwym indeksie odbiorcy
check: cargo test --test it t114_silent_step_named_for_reader::
expect: (\d+) passed

Kiedy udany krok oddał pustkę — wszystkie znormalizowane sekcje są puste — wiersz jego pliku
w **zmontowanym promcie następnego kroku** zachowuje dzisiejszą etykietę relacji i dostaje stały
angielski dopisek `left nothing`. Odpowiedź z choć jednym znakiem treści nie dostaje dopiska,
a jej wiersz jest co do bajta jak dziś. Test nie poprzestaje na helperze klasyfikującym ciało.

## AC-6 Tylko sędzia pętli musi mieć jedną kopię
check: cargo test --test it t114_only_loop_judge_runs_once::
expect: (\d+) passed

Gdy **źródło** strzałki powrotnej (`link.from`, kafelek zamykający pętlę) ma `copies > 1`,
sprawdzenie widoczne dla okna oraz `check_to_run` zwracają jedno angielskie zdanie z nazwą
kafelka, a zapis/start są odmawiane przed pierwszym procesem. Dwa powroty od tego samego sędziego
nie dublują zdania. Kontrole: `copies > 1` na zwykłym kroku oraz na **celu** strzałki powrotnej
pozostają legalne; sędzia z jedną kopią przechodzi.

<!-- OWNS
tasks/T-114.md
src-tauri/src/commands/run.rs
src-tauri/src/commands/isolate.rs
src-tauri/src/memory/handoff.rs
src-tauri/src/workflow/check.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/t114_copies_get_noncolliding_git_branches.rs
src-tauri/tests/it/t114_colliding_copy_refs_are_refused_before_start.rs
src-tauri/tests/it/t114_reader_gets_current_attachment_address.rs
src-tauri/tests/it/t114_last_decision_survives_limit.rs
src-tauri/tests/it/t114_silent_step_named_for_reader.rs
src-tauri/tests/it/t114_only_loop_judge_runs_once.rs
-->
