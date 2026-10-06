use std::path::PathBuf;

use dioxus::prelude::*;

use super::pick_folder;
use crate::settings::{detect_game_dir, detect_save_dir, is_game_dir, is_save_dir};
use crate::state::AppState;

#[derive(Clone, Copy, PartialEq)]
enum Folder {
    Saves,
    Game,
}

/// Folder locations, save watching, and undoing ignored runs.
#[component]
pub fn SettingsModal(on_close: EventHandler<()>) -> Element {
    let mut state = use_context::<AppState>();
    let settings = state.settings.read().clone();
    let watch = settings.tracking.watch_saves;

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div { class: "modal card", onclick: move |e| e.stop_propagation(),
                h2 { "Settings" }
                FolderField { folder: Folder::Saves, value: settings.paths.save_dir.clone() }
                FolderField { folder: Folder::Game, value: settings.paths.game_dir.clone() }
                label { class: "field field-inline",
                    input {
                        r#type: "checkbox",
                        checked: watch,
                        onchange: move |e| {
                            let on = e.checked();
                            state.update_settings(|s| s.tracking.watch_saves = on);
                        },
                    }
                    span { "Watch the save folder while the app is open" }
                }
                div { class: "field",
                    span { "Ignored playthroughs" }
                    button {
                        class: "btn btn-ghost",
                        onclick: move |_| state.unignore_runs(),
                        "Ask about ignored playthroughs again"
                    }
                }
                p { class: "muted small", "The tracker only ever reads your saves and game files; it never changes them." }
                div { class: "modal-actions",
                    button { class: "btn btn-primary", onclick: move |_| on_close.call(()), "Done" }
                }
            }
        }
    }
}

#[component]
fn FolderField(folder: Folder, value: Option<PathBuf>) -> Element {
    let mut state = use_context::<AppState>();
    let (label, hint, valid) = match folder {
        Folder::Saves => (
            "Save folder",
            "Usually Documents\\The Witcher 3\\gamesaves",
            value.as_deref().is_some_and(is_save_dir),
        ),
        Folder::Game => (
            "Game folder",
            "The folder containing \"content\" and \"bin\"",
            value.as_deref().is_some_and(is_game_dir),
        ),
    };
    let shown = value.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "Not set".into());
    let start = value.clone();

    rsx! {
        div { class: "field",
            span { "{label}" }
            div { class: "folder-row",
                code { class: if valid { "folder-path" } else { "folder-path invalid" }, "{shown}" }
                button {
                    class: "btn btn-ghost",
                    onclick: move |_| {
                        let start = start.clone();
                        spawn(async move {
                            if let Some(dir) = pick_folder(label, start).await {
                                set_folder(state, folder, dir);
                            }
                        });
                    },
                    "Browse…"
                }
                button {
                    class: "btn btn-ghost",
                    onclick: move |_| {
                        let found = match folder {
                            Folder::Saves => detect_save_dir(),
                            Folder::Game => detect_game_dir(),
                        };
                        match found {
                            Some(dir) => set_folder(state, folder, dir),
                            None => state.error.set(Some(format!("Could not find the {} automatically.", label.to_lowercase()))),
                        }
                    },
                    "Detect"
                }
            }
            span { class: "muted small", if valid { "{hint}" } else { "⚠ Not a valid {label.to_lowercase()}. {hint}" } }
        }
    }
}

fn set_folder(mut state: AppState, folder: Folder, dir: PathBuf) {
    state.update_settings(|s| match folder {
        Folder::Saves => s.paths.save_dir = Some(dir),
        Folder::Game => s.paths.game_dir = Some(dir),
    });
}
