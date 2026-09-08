//! Ograniczony czytelnik Context Leada: tylko wybór rozmowy, nigdy cała biblioteka.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use crate::bridge::host::Answers;
use crate::bridge::{Answer, Call};
use crate::commands::chat::{ChatPinSelection, ChatPins};
use crate::context::access::ContextAccess;
use crate::engine::line::RequestedContext;

#[derive(Clone)]
pub(super) struct LeadContextDesk {
    inner: Arc<Inner>,
}

struct Inner {
    home: PathBuf,
    project: PathBuf,
    conversation: Arc<Mutex<uuid::Uuid>>,
    selected: ChatPinSelection,
    expires: tokio_util::sync::CancellationToken,
    /// Krótki cache ograniczonego czytnika; zamek nigdy nie przeżywa await.
    cache: Mutex<Option<Cached>>,
}

struct Cached {
    generation: u64,
    access: ContextAccess,
}

impl LeadContextDesk {
    pub(super) fn new(
        home: PathBuf,
        project: PathBuf,
        conversation: Arc<Mutex<uuid::Uuid>>,
        selected: ChatPinSelection,
        expires: tokio_util::sync::CancellationToken,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                home,
                project,
                conversation,
                selected,
                expires,
                cache: Mutex::new(None),
            }),
        }
    }

    pub(super) fn selection(&self) -> ChatPinSelection {
        self.inner.selected.refreshed()
    }

    pub(super) fn preview(&self, selected: &ChatPins) -> Vec<RequestedContext> {
        selected
            .sets
            .clone()
            .into_iter()
            .map(|pin| {
                let title = crate::context::files::read_set(
                    &crate::context::files::library_root(&self.inner.home),
                    &pin.id,
                )
                .map_or_else(|_| pin.id.clone(), |read| read.set.title);
                RequestedContext {
                    id: pin.id,
                    title,
                    revision: pin.revision,
                    topics: pin.topics,
                }
            })
            .collect()
    }

    pub(super) async fn answer(&self, call: Call) -> Answer {
        let selected = self.inner.selected.current();
        if selected.sets.is_empty() {
            return empty_answer(&call);
        }
        let desk = self.clone();
        // 2026-09-08 (CT-07) — złożenie prywatnej paczki czyta i zapisuje pliki; most nie może
        // w tym czasie blokować innych rozmów ani ich czasowników.
        let access = tokio::task::spawn_blocking(move || desk.access(&selected)).await;
        match access {
            Err(error) => Answer::Refused(format!(
                "This selected Context could not be prepared for reading: {error}"
            )),
            Ok(Err(said)) => Answer::Refused(said),
            Ok(Ok(access)) => {
                /* 2026-09-08 (CT-07) — odmowa na granicy rozmowy nazywa jej przydział.
                 * Ogólny czytnik poprawnie odmawia obcego ID, lecz jego tekst mówi tylko o
                 * zamrożonym zestawie; Lead musi wiedzieć, że nie wolno mu rozszerzyć rozmowy. */
                if asks_for_unlisted_item(&access, &call) {
                    return Answer::Refused(
                        "This context item was not given to this conversation. Choose Context beside the message field first."
                            .to_owned(),
                    );
                }
                crate::bridge::context::ContextDesk::new(access)
                    .answer(call)
                    .await
            }
        }
    }

    fn access(&self, selected: &ChatPins) -> Result<ContextAccess, String> {
        if let Some(access) = self
            .inner
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|cached| cached.generation == selected.generation)
            .map(|cached| cached.access.clone())
        {
            return Ok(access);
        }
        let recipient = crate::commands::context_inputs::Recipient {
            node_key: "_lead",
            tile_key: "_lead",
            name: "Lead",
        };
        let prepared = crate::commands::context_inputs::compose(
            &self.inner.home,
            &selected.sets,
            &[recipient],
            true,
        )
        .map_err(|refusal| refusal.message)?;
        let conversation = self
            .inner
            .conversation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .to_string();
        // 2026-09-08 (CT-07) — osobny katalog zachowuje stary receipt bez nadpisania go wyborem
        // z późniejszej tury; całość pozostaje pod prywatną historią tej rozmowy.
        let folder = self
            .inner
            .project
            .join(".loadout/conversations")
            .join(conversation)
            .join("context-views")
            .join(uuid::Uuid::now_v7().to_string());
        let access = prepared
            .access_at(&folder, "_lead", self.inner.expires.clone())
            .map_err(|error| {
                format!("This selected Context could not be prepared for reading: {error}")
            })?
            .ok_or_else(|| "This conversation has no Context to read.".to_owned())?;
        *self
            .inner
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Cached {
            generation: selected.generation,
            access: access.clone(),
        });
        Ok(access)
    }
}

fn asks_for_unlisted_item(access: &ContextAccess, call: &Call) -> bool {
    if !matches!(call.call.as_str(), "read_context" | "view_context_image") {
        return false;
    }
    let Some(id) = call.input.get("id").and_then(serde_json::Value::as_str) else {
        return false;
    };
    access
        .list()
        .is_ok_and(|listed| listed.items.iter().all(|item| item.id != id))
}

fn empty_answer(call: &Call) -> Answer {
    if call.call == "list_context"
        && call
            .input
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
    {
        Answer::Ok(serde_json::json!({
            "setId": "chat",
            "revision": "none",
            "items": []
        }))
    } else {
        Answer::Refused(
            "This context item was not given to this conversation. Choose Context beside the message field first."
                .to_owned(),
        )
    }
}
