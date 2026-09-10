//! Atrapy dawnych scenariuszy wykonują tury. Dodajemy im osobny odczyt metadanych,
//! zanim zapiszą marker startu albo stdin tury. Model nieznany cennikowi może być dostępny.
use serde_json::json;
pub fn shell(body: &str) -> String {
    let names = [
        "opus",
        "sonnet",
        "haiku",
        "gpt-5-codex",
        "gpt-5.6-sol",
        "gpt-5.6-luna",
        "gpt-9.9-nebula",
    ];
    let codex: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(i, name)| json!({"model": name, "isDefault": i == 0}))
        .collect();
    let claude: Vec<_> = names
        .iter()
        .map(|name| json!({"value": name}))
        .chain(std::iter::once(json!({"value":"default"})))
        .collect();
    let codex = json!({"id":3,"result":{"data":codex,"nextCursor":null}});
    let claude = json!({"type":"control_response","response":{"request_id":"models","response":{"models":claude}}});
    let prefix = format!(
        r#"
# Metadane nie wykonują tury ani nie zapisują jej świadków.
metadata_only=false
previous_arg=
for one_arg in "$@"; do
  if [ "$previous_arg" = "--tools" ] && [ -z "$one_arg" ]; then metadata_only=true; fi
  previous_arg="$one_arg"
done
if [ "${{1-}}" = "app-server" ]; then
  while IFS= read -r request; do
    case "$request" in
      *'"method":"initialize"'*) printf '%s\n' '{{"id":1,"result":{{}}}}' ;;
      *'"method":"config/read"'*) printf '%s\n' '{{"id":2,"result":{{"config":{{}}}}}}' ;;
      *'"method":"model/list"'*) printf '%s\n' '{codex}'; exit 0 ;;
    esac
  done
  exit 0
fi
if [ "$metadata_only" = true ]; then
  IFS= read -r request
  printf '%s\n' '{claude}'
  exit 0
fi
"#
    );
    body.replacen("#!/bin/sh\n", &format!("#!/bin/sh\n{prefix}"), 1)
}
