// Release builds use the GUI subsystem so Windows doesn't open a console window alongside the
// app. Debug builds keep the console for tracing output.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
mod components;
mod i18n;
mod item_sync;
mod save_tracker;
mod settings;
mod state;
mod sync_service;
mod view_options;
mod workbench;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

use components::quests::QuestsView;
use components::shell::{DetectedRuns, Header, Onboarding, OnboardingStep, SaveWatcher, SyncBanner, Toasts};
use components::workbench::WorkbenchView;
use settings::Tab;
use state::AppState;

/// Rendered from `assets/logo.svg` by `scripts/render_icons.py`.
const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

fn main() {
    tracing_subscriber::fmt::init();

    let window = WindowBuilder::new()
        .with_title("Witcher 3 Companion")
        .with_inner_size(LogicalSize::new(1440.0, 900.0))
        .with_window_icon(dioxus::desktop::icon_from_memory(ICON_PNG).ok());

    LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window).with_menu(None))
        .launch(App);
}

/// Opens (creating if needed) the DB in the OS data directory, with the settings file next to it.
/// `W3C_DATA_DIR` (or the older `W3QT_DATA_DIR`) overrides the directory, for trying things out
/// without touching real data.
fn init_state() -> Result<AppState, String> {
    let dir = match std::env::var_os("W3C_DATA_DIR").or_else(|| std::env::var_os("W3QT_DATA_DIR")) {
        Some(dir) => std::path::PathBuf::from(dir),
        None => {
            let data = dirs::data_dir().ok_or("Could not determine the OS data directory")?;
            let dir = data.join("witcher3_companion");
            migrate_legacy_dir(&data.join("witcher3_quest_tracker"), &dir);
            dir
        }
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
    let db_path = dir.join("companion.db");
    tracing::info!("Using database at {}", db_path.display());
    let conn = companion_db::open(&db_path).map_err(|e| e.to_string())?;
    Ok(AppState::new(conn, &dir.join("settings.toml")))
}

/// Copies the data of the app's former incarnation (Witcher 3 Quest Tracker) into the new data
/// directory on first launch. The old directory is left in place.
fn migrate_legacy_dir(old: &std::path::Path, new: &std::path::Path) {
    if new.exists() || !old.join("quests.db").exists() {
        return;
    }
    let copy = || -> std::io::Result<()> {
        std::fs::create_dir_all(new)?;
        for (from, to) in [
            ("quests.db", "companion.db"),
            ("quests.db-wal", "companion.db-wal"),
            ("quests.db-shm", "companion.db-shm"),
            ("settings.toml", "settings.toml"),
        ] {
            if old.join(from).exists() {
                std::fs::copy(old.join(from), new.join(to))?;
            }
        }
        Ok(())
    };
    match copy() {
        Ok(()) => tracing::info!("Migrated data from {}", old.display()),
        Err(e) => {
            tracing::warn!("Could not migrate data from {}: {e}", old.display());
            // Start clean rather than with a half-copied database.
            let _ = std::fs::remove_dir_all(new);
        }
    }
}

#[component]
fn App() -> Element {
    let init = use_hook(init_state);

    rsx! {
        style { {assets::STYLE} }
        div { class: "backdrop" }
        match init {
            Ok(state) => rsx! { Shell { state } },
            Err(e) => rsx! {
                div { class: "fatal panel",
                    h1 { "Witcher 3 Companion" }
                    p { "Could not open the database." }
                    p { class: "muted", "{e}" }
                }
            },
        }
    }
}

#[component]
fn Shell(state: AppState) -> Element {
    let mut state = use_context_provider(|| state);
    assets::use_asset_routes(state);
    // Lets the user continue without game data this session.
    let mut sync_skipped = use_signal(|| false);

    // Persist the quest and workbench filters whenever they change (search text is not saved,
    // so typing in a search box does not write the file).
    let snapshot = move || {
        let view = toml::to_string(&*state.view.read()).unwrap_or_default();
        let bench = toml::to_string(&*state.bench.read()).unwrap_or_default();
        view + &bench
    };
    let mut saved = use_hook(|| CopyValue::new(String::new()));
    use_effect(move || {
        let current = snapshot();
        if saved.peek().is_empty() {
            saved.set(current);
        } else if *saved.peek() != current {
            state.save_settings();
            saved.set(current);
        }
    });

    let settings = state.settings.read();
    let save_dir = settings.valid_save_dir().map(|d| d.display().to_string());
    let game_dir_ok = settings.valid_game_dir().is_some();
    let watch = settings.tracking.watch_saves;
    let language = i18n::language(&settings.ui.language);
    drop(settings);
    let has_data = *state.quest_count.read() > 0 || *state.item_count.read() > 0;
    let tab = *state.tab.read();

    let step = if save_dir.is_none() {
        Some(OnboardingStep::SaveDir)
    } else if !game_dir_ok {
        Some(OnboardingStep::GameDir)
    } else if !has_data && !sync_skipped() {
        Some(OnboardingStep::Sync)
    } else {
        None
    };

    rsx! {
        div {
            class: "app",
            lang: language.tag,
            dir: if language.rtl() { "rtl" } else { "ltr" },
            "data-script": language.script.attr(),
            if let Some(step) = step {
                Onboarding { step, on_skip_sync: move |_| sync_skipped.set(true) }
            } else {
                Header {}
                SyncBanner {}
                match tab {
                    Tab::Quests => rsx! { QuestsView {} },
                    Tab::Workbench => rsx! { WorkbenchView {} },
                }
                if let Some(dir) = save_dir.clone() {
                    if has_data {
                        SaveWatcher { key: "{dir}-{watch}", dir, watch }
                    }
                }
                DetectedRuns {}
            }
            Toasts {}
        }
    }
}
