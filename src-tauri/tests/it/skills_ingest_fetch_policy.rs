//! AC-4 dla T-19: polityka adresu odrzuca to, co ma odrzucać, nie daje się oszukać kształtem
//! hosta, a limity są sprawdzane u siebie, nie zadeklarowane w argv `curl`-a.
//!
//! **Słabą wersją tego kryterium jest jeden przypadek z `http://`.** Przechodzi go
//! implementacja porównująca `url.contains("github.com")` — a to porównanie przepuszcza OBA
//! adresy myszkujące kształtem hosta: `github.com.evil.tld` (cudza domena z naszą jako
//! prefiksem) i `evil.tld/x?u=github.com/o/r` (nasza nazwa w parametrze). Oba są tu, i oba
//! muszą być odmową.
//!
//! Druga rzecz, którą to kryterium pilnuje, to niezmiennik 20 w jego najczystszej postaci:
//! flagi narzędzia nie są dowodem. `--max-redirs 3` w argv jest deklaracją `curl`-a
//! i sprawdzamy, że stoi — ale łańcuch przekierowań, rozmiar pliku i suma rozmiarów są
//! sprawdzane JESZCZE RAZ u nas, na tym, co faktycznie przyszło. Dokładnie tak umarło
//! `--sandbox workspace-write` w spreadsheecie: flaga stała w komentarzu, żywa brzmiała
//! `danger-full-access`, a test asertował obecność napisu (raport 06).
//!
//! Wszystko offline. Sieć żyje w aplikacji; bramka, która wymaga internetu, jest bramką
//! czerwieniejącą od cudzych awarii.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `panic!` dochodzi do tej listy z tego samego powodu (`[workspace.lints]` ma `panic = "deny"`
// dla całego drzewa): gałąź `other => panic!(…)` cytuje to, co WRÓCIŁO, a asercja, która nie
// mówi, co dostała zamiast oczekiwanego kształtu, każe uruchamiać test drugi raz pod debugerem.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Cursor;

use loadout_lib::skills::ingest::{self, FILE_CAP, FetchError, TOTAL_CAP, Target};

/// Adres, który wygląda jak z listy, i nie jest: `github.com` jest tu PREFIKSEM cudzej domeny.
const LOOKALIKE_HOST: &str = "https://github.com.evil.tld/o/r";

/// Adres, w którym nasza nazwa siedzi w parametrze zapytania, a host jest cudzy.
const NAME_IN_QUERY: &str = "https://evil.tld/x?u=github.com/o/r";

/// Czym skończyło się rozwiązanie adresu — zdaniem, które da się wkleić do komunikatu asercji.
fn outcome(url: &str) -> String {
    match ingest::resolve_url(url) {
        Ok(target) => format!("<accepted as {target:?}>"),
        Err(error) => format!("<refused: {error}>"),
    }
}

#[test]
fn plain_http_is_refused_even_on_a_host_from_the_list() {
    let url = "http://raw.githubusercontent.com/o/r/main/SKILL.md";
    assert!(
        matches!(ingest::resolve_url(url), Err(FetchError::NotHttps)),
        "{url} came back {}. The bytes fetched here become instructions an agent will follow, \
         so the channel cannot be one that anybody on the path can write into",
        outcome(url)
    );
}

#[test]
fn a_host_that_only_looks_like_the_list_is_refused() {
    for url in [LOOKALIKE_HOST, NAME_IN_QUERY] {
        assert!(
            matches!(ingest::resolve_url(url), Err(FetchError::HostNotAllowed)),
            "{url} came back {}. The host has to be compared WHOLE against the list; \
             `contains(\"github.com\")` accepts this address and every other one that carries \
             our name somewhere in it",
            outcome(url)
        );
    }
}

#[test]
fn the_three_shapes_we_do_accept_are_read_apart() {
    let raw = "https://raw.githubusercontent.com/anthropics/skills/main/skills/pdf/SKILL.md";
    assert!(
        matches!(ingest::resolve_url(raw), Ok(Target::RawFile)),
        "a raw SKILL.md is fetched directly [T5 §5.1], and this one came back {}",
        outcome(raw)
    );

    let folder = "https://github.com/o/r/tree/main/skills/pdf";
    match ingest::resolve_url(folder) {
        Ok(Target::Folder {
            owner,
            repo,
            git_ref,
            path,
        }) => assert_eq!(
            (
                owner.as_str(),
                repo.as_str(),
                git_ref.as_str(),
                path.as_str()
            ),
            ("o", "r", "main", "skills/pdf"),
            "a subfolder link has to come apart into the four pieces the contents API needs. \
             Recognising the shape and not reading it is how the fetch ends up asking for the \
             repository root"
        ),
        other => panic!("{folder} came back {other:?}, and it is a folder [T5 §5.1]"),
    }

    let gist = "https://gist.github.com/u/abc";
    assert!(
        matches!(ingest::resolve_url(gist), Ok(Target::Gist)),
        "a gist is its own shape [T5 §5.1], and this one came back {}",
        outcome(gist)
    );
}

#[test]
fn a_redirect_chain_is_checked_link_by_link_not_only_at_the_start() {
    // Zaczyna się na hoście z listy i kończy poza nią. To jest cała treść tej klasy ataku:
    // dozwolony adres oddaje treść z niedozwolonego, a sprawdzenie tylko pierwszego ogniwa
    // przepuszcza dowolną stronę w internecie.
    let leaves = [
        "https://raw.githubusercontent.com/o/r/main/SKILL.md",
        "https://cdn.evil.tld/skill.md",
    ];
    assert!(
        matches!(
            ingest::follow_policy(&leaves),
            Err(FetchError::HostNotAllowed)
        ),
        "a chain that starts on the list and ends off it was allowed: {:?}",
        ingest::follow_policy(&leaves).err().map(|e| e.to_string())
    );

    // Cztery przeskoki, wszystkie na hostach z listy. Odmowa jest o LICZBIE, nie o hoście.
    let four_hops = [
        "https://github.com/o/r",
        "https://github.com/o/r/tree/main",
        "https://github.com/o/r/tree/main/skills",
        "https://github.com/o/r/tree/main/skills/pdf",
        "https://raw.githubusercontent.com/o/r/main/skills/pdf/SKILL.md",
    ];
    assert!(
        matches!(
            ingest::follow_policy(&four_hops),
            Err(FetchError::TooManyRedirects)
        ),
        "four hops is one more than the policy allows [T5 §5.2], and every one of these hosts \
         is on the list, so nothing else can be doing the refusing"
    );

    // Kierunek drugi: polityka, która odmawia zawsze, jest tak samo bezużyteczna jak żadna.
    let ordinary = [
        "https://github.com/o/r/tree/main/skills/pdf",
        "https://raw.githubusercontent.com/o/r/main/skills/pdf/SKILL.md",
    ];
    assert!(
        ingest::follow_policy(&ordinary).is_ok(),
        "one hop between two hosts on the list is what an ordinary GitHub fetch looks like"
    );
}

#[test]
fn one_byte_over_the_file_limit_is_refused_and_the_limit_is_in_the_sentence() {
    let over = Cursor::new(vec![0u8; 1_048_577]);
    let refusal = ingest::read_capped(over, FILE_CAP)
        .err()
        .map(|error| (format!("{error:?}"), error.to_string()));

    let Some((debug, said)) = refusal else {
        panic!(
            "a file one byte over the limit was read to the end. The limit exists because a \
                skill is a handful of kilobytes and anything else is somebody using the fetch \
                as a way to fill the disk"
        );
    };
    assert!(
        debug.starts_with("FileTooBig"),
        "the refusal has to name this cause and not another: {debug}"
    );
    assert!(
        said.contains(&FILE_CAP.to_string()),
        "`{said}` does not say what the limit was. `file too big` without the number leaves the \
         person guessing by how much, which is the difference between trimming a reference file \
         and giving up"
    );

    // Kierunek drugi: dokładnie na limicie to jeszcze nie za dużo, i ma wrócić w całości.
    let exact = ingest::read_capped(Cursor::new(vec![0u8; 1_048_576]), FILE_CAP)
        .expect("a file exactly at the limit is within the limit");
    assert_eq!(
        exact.len(),
        1_048_576,
        "the bytes we accepted have to be all of them: a reader that stops one short hands the \
         parser half a file and the failure surfaces as `invalid frontmatter`"
    );
}

#[test]
fn five_files_that_each_fit_can_still_be_too_much_together() {
    // 1,2 MB każdy: pojedynczo mieszczą się w żadnym sensownym limicie strony, a razem to
    // sześć megabajtów na umiejętność, która ma być folderem z tekstem.
    let too_much = [1_258_291_u64; 5];
    assert!(
        matches!(
            ingest::total_within(&too_much, TOTAL_CAP),
            Err(FetchError::TotalTooBig { .. })
        ),
        "five files of 1.2 MB are 6 MB together, and the total limit is 5 MB [T5 §5.2]. A \
         per-file limit alone is a limit on nothing: the same bytes arrive as more files"
    );

    let fits = [943_718_u64; 5];
    assert_eq!(
        ingest::total_within(&fits, TOTAL_CAP).ok(),
        Some(4_718_590),
        "and five files of 0.9 MB are under the total, so they go through with their sum \
         reported back"
    );
}

#[test]
fn the_fetch_command_carries_the_limits_it_is_supposed_to_carry() {
    let command = ingest::build_fetch_command("https://raw.githubusercontent.com/o/r/m/SKILL.md");
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    for flag in [
        "--proto",
        "=https",
        "--max-redirs",
        "3",
        "--max-filesize",
        "--max-time",
        "20",
    ] {
        assert!(
            args.iter().any(|arg| arg == flag),
            "`{flag}` is missing from {args:?}. These are the first line of defence and NOT the \
             proof — the same run re-checks every one of them on what actually arrived, because \
             a flag in a comment next to a live flag saying something else is exactly how this \
             class of check dies (invariant 20)"
        );
    }
}
