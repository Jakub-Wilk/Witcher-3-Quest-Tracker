// Release builds use the GUI subsystem so Windows doesn't open a console window alongside the
// app. Debug builds keep the console for tracing output.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod components;
mod save_tracker;
mod settings;
mod state;
mod sync_service;
mod view_options;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

use components::{
    CutoffPane, DetectedRuns, Header, Onboarding, OnboardingStep, QuestList, SaveWatcher, Sidebar,
};
use state::AppState;

/// Inlined rather than loaded via `asset!` so a plain `cargo run` works without the `dx` CLI.
const STYLE: &str = include_str!("../assets/style.css");

/// Rendered from `assets/logo.svg` by `scripts/render_icons.py`.
const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

fn main() {
    tracing_subscriber::fmt::init();

    let window = WindowBuilder::new()
        .with_title("Witcher 3 Quest Tracker")
        .with_inner_size(LogicalSize::new(1440.0, 900.0))
        .with_window_icon(dioxus::desktop::icon_from_memory(ICON_PNG).ok());

    LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window).with_menu(None))
        .launch(App);
}

/// Opens (creating if needed) the DB in the OS data directory, with the settings file next to it.
/// `W3QT_DATA_DIR` overrides the directory (for trying things out without touching real data).
fn init_state() -> Result<AppState, String> {
    let dir = match std::env::var_os("W3QT_DATA_DIR") {
        Some(dir) => std::path::PathBuf::from(dir),
        None => dirs::data_dir()
            .ok_or("Could not determine the OS data directory")?
            .join("witcher3_quest_tracker"),
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
    let db_path = dir.join("quests.db");
    tracing::info!("Using database at {}", db_path.display());
    let conn = quest_db::open(&db_path).map_err(|e| e.to_string())?;
    Ok(AppState::new(conn, &dir.join("settings.toml")))
}

#[component]
fn App() -> Element {
    let init = use_hook(init_state);

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
    // Lets the user continue without quest data this session.
    let mut sync_skipped = use_signal(|| false);

    // Persist filters and sort order whenever they change (search text is not saved, so
    // typing in the search box does not write the file).
    let mut saved = use_hook(|| CopyValue::new(toml::to_string(&*state.view.peek()).unwrap_or_default()));
    use_effect(move || {
        let current = toml::to_string(&*state.view.read()).unwrap_or_default();
        if *saved.peek() != current {
            state.save_settings();
            saved.set(current);
        }
    });

    let settings = state.settings.read();
    let save_dir = settings.valid_save_dir().map(|d| d.display().to_string());
    let game_dir_ok = settings.valid_game_dir().is_some();
    let watch = settings.tracking.watch_saves;
    drop(settings);
    let has_quests = *state.quest_count.read() > 0;

    let step = if save_dir.is_none() {
        Some(OnboardingStep::SaveDir)
    } else if !game_dir_ok {
        Some(OnboardingStep::GameDir)
    } else if !has_quests && !sync_skipped() {
        Some(OnboardingStep::Sync)
    } else {
        None
    };

    rsx! {
        div { class: "app",
            if let Some(step) = step {
                Onboarding { step, on_skip_sync: move |_| sync_skipped.set(true) }
            } else {
                Header {}
                div { class: "panes",
                    Sidebar {}
                    QuestList {}
                    CutoffPane {}
                }
                if let Some(dir) = save_dir.clone() {
                    if has_quests {
                        SaveWatcher { key: "{dir}-{watch}", dir, watch }
                    }
                }
                DetectedRuns {}
            }
            if let Some(notice) = state.notice.read().clone() {
                div { class: "toast toast-info",
                    span { "{notice}" }
                    button { class: "toast-close", onclick: move |_| state.notice.set(None), "✕" }
                }
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
