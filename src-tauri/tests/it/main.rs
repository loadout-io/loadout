//! Jeden cel testowy zamiast 122 — i to jest zmiana WYDAJNOŚCIOWA o zmierzonym powodzie.
//!
//! Rust robi z każdego pliku w `tests/` OSOBNE binarium, a każde takie binarium statycznie
//! linkuje całą bibliotekę razem z 527 skrzyniami zależności. Zmierzone 2026-08-17: same testy
//! wykonują się w **6,0 s**, a składanie tych 122 programów zajmowało godziny — `full-test`
//! przekraczał budżet 1800 s dwukrotnie i od 2026-08-17 nie wylądowałaby żadna gałąź.
//!
//! Pliki w PODKATALOGU `tests/it/` nie są celami: są modułami tego jednego celu. Ta sama
//! liczba testów, te same asercje, jeden link.
//!
//! Dla porównania, zmierzone na tej maszynie: `../meetnotes` ma **950** skrzyń (prawie dwa razy
//! więcej niż my) i JEDNO binarium testowe — 19 835 plików w `target/debug/deps`. Nasze
//! 122 cele dały **886 645** plików i 66 GB.
//!
//! JAK URUCHOMIĆ POJEDYNCZY ZESTAW. Nazwa pliku stała się nazwą modułu, więc
//! `cargo test --test it store_pragmas::` robi dokładnie to, co robiło
//! `cargo test --test store_pragmas`. Filtrowanie po nazwie testu działa jak dotąd.
//!
//! CO ZOSTAJE OSOBNYM CELEM, i to nie jest wyjątek dla wygody. Test, który mierzy albo zmienia
//! stan CAŁEGO PROCESU, w scalonym binarium mierzy 285 cudzych testów naraz. Zmierzone
//! 2026-08-17, przy pierwszym lądowaniu po scaleniu: `shell_logging` liczy otwarte deskryptory
//! przez `/dev/fd` i instaluje globalny hak paniki — dostał 96 zamiast swojej liczby, bo
//! sąsiedzi otwierali pliki w tej samej chwili. `supervisor_env_hygiene` woła `env::set_var`,
//! co jest zmienną globalną procesu. Oba mieszkają więc w `tests/` jako własne cele; koszt to
//! dwa linkowania zamiast stu dwudziestu dwóch.
//!
//! DOPISANIE NOWEGO PLIKU wymaga jednej linii `mod` niżej. To jest cena tej zmiany i jest
//! świadoma: plik bez wpisu kompiluje się do niczego i nie uruchamia ani jednego testu —
//! czyli wygląda dokładnie jak zestaw, który przeszedł. Pilnuje tego `checks/quick-tests-listed.sh`.

mod agents_capture;
mod agents_file_format;
mod agents_resolve;
mod agents_vendor_args_filtered;
mod agents_vendor_args_one_policy;
mod agents_vendor_options;
mod agents_wire_shape;
mod chat_never_starts_a_run;
mod claude_argv_policy;
mod claude_argv_transport;
mod claude_cancel_escalation;
mod claude_completion;
mod claude_rate_limit;
mod claude_session_process;
mod claude_unknown_events;
mod close_stops_the_run;
mod engine_cancel_outcome;
mod engine_concurrency_limit;
mod engine_cone_reason;
mod engine_dag_construction;
mod engine_order;
mod engine_overlap;
mod engine_step_states;
mod fresh_copy_degrades_loudly;
mod fresh_copy_isolates_steps;
mod harness_workflow_chain;
mod harness_workflow_findings_match_doc;
mod harness_workflow_sequential;
mod harness_workflow_two_kinds;
mod harness_workflow_validates;
mod harness_workflow_vendor_pairing;
mod ipc_commands_registered;
mod ipc_library_roundtrip;
mod ipc_line_wire_golden;
mod ipc_pump_backpressure;
mod ipc_pump_cap;
mod ipc_pump_lifecycle;
mod ipc_pump_order;
mod ipc_pump_timer;
mod ipc_read_paths;
mod ipc_workflow_roundtrip;
mod limits_are_global_across_runs;
mod limits_dial_lowers;
mod limits_dial_raises;
mod limits_pause_is_run_level;
mod limits_pause_on_rate_limit;
mod limits_rate_limit_status;
mod limits_resume_at_reset;
mod limits_suggested_at_once;
mod memory_handoff_cap;
mod memory_handoff_frontmatter;
mod memory_handoff_paths;
mod memory_handoff_scan;
mod memory_handoff_sections;
mod memory_handoff_supersede;
mod memory_notes_because;
mod memory_notes_budget;
mod memory_notes_files;
mod memory_notes_injection;
mod memory_notes_promotion;
mod person_turn_is_visible;
mod product_path_end_to_end;
mod recovery_asks_never_resumes;
mod recovery_boot_guard;
mod recovery_proof_of_death;
mod recovery_reap_targets;
mod recovery_records_boot_time;
mod recovery_runs_at_startup;
mod recovery_status_table;
mod recovery_unreadable_rows;
mod run_commands_registered;
mod run_reaches_the_pump;
mod run_stop_waits_for_proof;
mod runcmd_cancel;
mod runcmd_checkpoint;
mod runcmd_end_to_end;
mod runcmd_parallel;
mod runcmd_refuses_invalid;
mod runcmd_snapshot;
mod say_to_agent_refusals;
mod skeleton_group_death;
mod skeleton_two_real_agents;
mod skills_author_origin;
mod skills_author_pipeline;
mod skills_draft_asks_an_agent;
mod skills_draft_stops_dead;
mod skills_ingest_clean;
mod skills_ingest_fetch_policy;
mod skills_ingest_injection;
mod skills_ingest_no_exec;
mod skills_ingest_scanner;
mod skills_ingest_selftest;
mod skills_place_destinations;
mod skills_place_discovery;
mod skills_place_emit;
mod skills_place_plan;
mod skills_place_remove;
mod skills_place_validate;
mod skills_scope_round_trip;
mod skills_scope_two_roots;
mod step_deadline_stops_the_agent;
mod store_append_only;
mod store_batch_atomic;
mod store_disposable;
mod store_migrate_idempotent;
mod store_pragmas;
mod store_single_writer;
mod store_strict_schema;
mod stream_closing_lines;
mod stream_coalesce_window;
mod stream_collapse_defaults;
mod stream_curation_fixture;
mod stream_live_curation;
mod stream_raw_tee;
mod stream_raw_tee_live;
mod stream_tee_survives_db_delete;
mod stream_thinking_slot;
mod stream_unknown_events;
mod supervisor_drop_guard;
mod supervisor_group_death;
mod supervisor_pipe_eof;
mod supervisor_term_then_kill;
mod supervisor_timeout_kills;
mod workflow_check_cycles;
mod workflow_check_ids;
mod workflow_check_islands;
mod workflow_check_overlap;
mod workflow_load_forward;
mod workflow_loop_back_edge;
mod workflow_reserved_flags;
mod workflow_save_refuses;
mod workflow_unknown_keys;
mod workspace_global_slots;
mod workspace_registry;
mod workspace_switch_keeps_runs;
