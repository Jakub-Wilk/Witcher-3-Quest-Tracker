mod components;
mod state;
mod sync_service;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

use components::{CutoffPane, Header, QuestList, Sidebar};
use state::AppState;

/// Inlined rather than loaded via `asset!` so a plain `cargo run` works without the `dx` CLI.
const STYLE: &str = include_str!("../assets/style.css");

fn main() {
    tracing_subscriber::fmt::init();

    let window = WindowBuilder::new()
        .with_title("Witcher 3 Quest Tracker")
        .with_inner_size(LogicalSize::new(1440.0, 900.0));

    LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window).with_menu(None))
        .launch(App);
}

/// Opens (creating if needed) the DB in the OS data directory.
fn open_db() -> Result<quest_db::Connection, String> {
    let dir = dirs::data_dir()
        .ok_or("Could not determine the OS data directory")?
        .join("witcher3_quest_tracker");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
    let path = dir.join("quests.db");
    tracing::info!("Using database at {}", path.display());
    quest_db::open(&path.to_string_lossy()).map_err(|e| e.to_string())
}

#[component]
fn App() -> Element {
    let init = use_hook(|| open_db().map(AppState::new));

    rsx! {
        style { {STYLE} }
        match init {
            Ok(state) => rsx! { Shell { state } },
            Err(e) => rsx! {
                div { class: "fatal",
                    h1 { "Could not open the quest database" }
                    p { "{e}" }
                }
            },
        }
    }
}

#[component]
fn Shell(state: AppState) -> Element {
    let mut state = use_context_provider(|| state);

    rsx! {
        div { class: "app",
            Header {}
            div { class: "panes",
                Sidebar {}
                QuestList {}
                CutoffPane {}
            }
            if let Some(err) = state.error.read().clone() {
                div { class: "toast toast-error",
                    span { "{err}" }
                    button { class: "toast-close", onclick: move |_| state.error.set(None), "✕" }
                }
            }
        }
    }
}
