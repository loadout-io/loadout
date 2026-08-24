# T-98 — Przelotka nie sięga ponad dial: pełne listy i filtr po kluczach

Do T-90 przelotka vendorowa (`vendorOptions`, D6) nie docierała do argv wcale, więc dziury
w listach zarezerwowanych nie miały skutku. Od T-90 dociera — i stan dzisiejszy jest taki
(zweryfikowane w trunku 2026-08-24): `RESERVED_CLAUDE` ma 8 pozycji, `RESERVED_CODEX` 4,
a `FORBIDDEN_ESCALATIONS` to trzy podciągi skanowane po nazwie i wartości. Przechodzi więc:
`--settings <własny plik>` (czyli podmiana pliku, którym T-92 przekierowuje auto-pamięć
i wnosi reguły deny gospodarza), `--add-dir`, `--mcp-config`, `--plugin-dir`, `--tools`,
`--allowedTools`, `--disallowedTools`, `--append-system-prompt`, `--model`,
`--max-budget-usd`, `--resume`, `--continue`, `--agents`, `--permission-prompt-tool`;
po stronie Codeksa `-c sandbox_mode=workspace-write` (podniesienie z „look-only", bo filtr
zna tylko literał `danger-full-access`), `-c approval_policy=…`, `-c mcp_servers.x.command=…`,
`-c model_provider=…` i `-c model_providers.*.base_url=…` (eksfiltracja promptu).

Mechanizm jest dobry i działa — `--effort` dopisało T-90 po zgłoszeniu pisarza T-91 —
brakuje mu wyłącznie pozycji oraz dopasowania po KLUCZU zamiast po podciągu.

**Mandat właściciela (D-1, 2026-08-24):** `agents_vendor_args_filtered.rs` używa dziś
`--settings` jako przykładu flagi NIEzarezerwowanej. Ta przesłanka zmienia się świadomie —
test dostaje inny przykład flagi wolnej (np. `--verbose-tool-output`), asercje pozostają
tak samo mocne.

**Read first:** `src-tauri/src/workflow/check.rs` (`RESERVED_CLAUDE` ok. 45, `RESERVED_CODEX`
ok. 58, `FORBIDDEN_ESCALATIONS` ok. 77, `reserved(vendor)` ok. 795) ·
`src-tauri/src/library/agents.rs` (`vendor_args_filtered` ok. 890, `passthrough_of_the_step`,
`vendor_argv`) · `src-tauri/tests/it/agents_vendor_args_filtered.rs` i
`src-tauri/tests/it/workflow_reserved_flags.rs` (istniejące wyrocznie — rozszerz, nie dubluj) ·
`docs/PLAN-HARDENING.md` §3 (zakres) · `AGENTS.md` niezmienniki 16, 21.

## Kto to robi

- **Agent:** `rust-core`. Bez `commands/run.rs` — zadanie może biec równolegle z każdym
  poza T-99 (wspólny `workflow/check.rs`; T-99 zależy od tego zadania).
- **Druga opinia:** inny vendor niż pisarz (D3).

## AC-1 Każda flaga, którą Loadout ustawia sam u Claude'a, jest odmową przelotki
check: cargo test --test it reserved_flags_cover_what_loadout_sets::
expect: (\d+) passed

`RESERVED_CLAUDE` obejmuje co najmniej: dotychczasowe osiem plus `--settings`, `--add-dir`,
`--mcp-config`, `--plugin-dir`, `--tools`, `--allowedTools`, `--disallowedTools`,
`--append-system-prompt`, `--model`, `--max-budget-usd`, `--resume`, `--continue`,
`--agents`, `--permission-prompt-tool`. Przelotka z każdą z nich to odmowa zapisu/planu
z nazwaniem flagi (jak dziś). Kontrola: flaga spoza listy (np. `--verbose-tool-output`)
przechodzi bez zmian. Kryterium sądzi ZAWARTOŚĆ listy przeciw liście flag budowanych
w `claude.rs::command` — nie przepisuje jej drugi raz, tylko dowodzi pokrycia na próbce
wymienionej wyżej.

## AC-2 Klucze `-c`, którymi Codex podnosi uprawnienia, są odmową po prefiksie
check: cargo test --test it codex_config_keys_are_reserved::
expect: (\d+) passed

`RESERVED_CODEX` (albo osobna lista prefiksów obok) łapie po prefiksie klucza przed `=`:
`sandbox_mode`, `sandbox_workspace_write.network_access`, `approval_policy`,
`mcp_servers.`, `model_provider`, `model_providers.`. `-c mcp_servers.x.command=/bin/sh`
i `-c model_providers.custom.base_url=…` są odmową z nazwaniem klucza;
`model_reasoning_effort` pozostaje odmową jak dziś. Kontrola: klucz spoza list
(np. `-c profile=ci`) przechodzi.

## AC-3 Filtr podniesień dopasowuje po regule, nie po samym podciągu
check: cargo test --test it escalations_match_by_key::
expect: (\d+) passed

`FORBIDDEN_ESCALATIONS` dostaje pozycję dla kolizji z `--max-budget-usd` (dług T-94,
`docs/STATUS.md`), a dopasowanie flag idzie po kluczu/prefiksie zamiast `contains` na
całości. Wartości nadal są skanowane o trzy dotychczasowe literały — wartość niosąca
`danger-full-access` w dowolnym kluczu pozostaje odmową (kontrola).

## AC-4 Stara wyrocznia stoi na nowej przesłance
check: cargo test --test it agents_vendor_args_filtered::
expect: (\d+) passed

`agents_vendor_args_filtered.rs` po zmianie przykładu (mandat D-1 wyżej) przechodzi
w całości: przykład flagi wolnej nie jest żadną z pozycji AC-1, a `--settings` jest
w teście po stronie ODMÓW.

## Sprzątanie po drodze

Komentarz nad `RESERVED_CLAUDE` mówi, że kolizja „do tego zadania nie miała skutku, bo
przelotka nie dojeżdżała do argv" — od T-90 dojeżdża; dopisz zdanie z datą i tym zadaniem.

<!-- OWNS
tasks/T-98.md
src-tauri/src/workflow/check.rs
src-tauri/src/library/agents.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/agents_vendor_args_filtered.rs
src-tauri/tests/it/workflow_reserved_flags.rs
src-tauri/tests/it/reserved_flags_cover_what_loadout_sets.rs
src-tauri/tests/it/codex_config_keys_are_reserved.rs
src-tauri/tests/it/escalations_match_by_key.rs
-->
