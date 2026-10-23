use std::path::PathBuf;

use dioxus::prelude::*;

use crate::components::ornament::Divider;
use crate::components::pick_folder;
use crate::settings::{detect_game_dir, detect_save_dir, is_game_dir, is_save_dir};
use crate::state::AppState;
use crate::t;

#[derive(Clone, Copy, PartialEq)]
enum Folder {
    Saves,
    Game,
}

/// Folder locations, save watching, and undoing ignored runs.
#[component]
pub fn SettingsModal(on_close: EventHandler<()>) -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let settings = state.settings.read().clone();
    let watch = settings.tracking.watch_saves;

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div { class: "modal panel", onclick: move |e| e.stop_propagation(),
                h2 { class: "modal-title", {t!(tr, "settings.title")} }
                Divider {}
                FolderField { folder: Folder::Saves, value: settings.paths.save_dir.clone() }
                FolderField { folder: Folder::Game, value: settings.paths.game_dir.clone() }
                label { class: "toggle",
                    input {
                        r#type: "checkbox",
                        checked: watch,
                        onchange: move |e| {
                            let on = e.checked();
                            state.update_settings(|s| s.tracking.watch_saves = on);
                        },
                    }
                    span { class: "toggle-track", span { class: "toggle-thumb" } }
                    span { {t!(tr, "settings.watch")} }
                }
                p { class: "muted small", {t!(tr, "settings.watch_hint")} }
                div { class: "field",
                    span { {t!(tr, "settings.ignored")} }
                    button { class: "btn btn-ghost", onclick: move |_| state.unignore_runs(), {t!(tr, "settings.unignore")} }
                }
                p { class: "muted small", {t!(tr, "settings.read_only")} }
                div { class: "modal-actions",
                    button { class: "btn btn-gold", onclick: move |_| on_close.call(()), {t!(tr, "common.done")} }
                }
            }
        }
    }
}

#[component]
fn FolderField(folder: Folder, value: Option<PathBuf>) -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let (label, hint, valid) = match folder {
        Folder::Saves => (t!(tr, "settings.save_folder"), t!(tr, "settings.save_folder_hint"), value.as_deref().is_some_and(is_save_dir)),
        Folder::Game => (t!(tr, "settings.game_folder"), t!(tr, "settings.game_folder_hint"), value.as_deref().is_some_and(is_game_dir)),
    };
    let shown = value.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| t!(tr, "settings.not_set").into());
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
                    {t!(tr, "common.browse")}
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
                            None => state.error.set(Some(t!(tr, "settings.detect_failed", folder = label))),
                        }
                    },
                    {t!(tr, "settings.detect")}
                }
            }
            span { class: "muted small",
                if valid { "{hint}" } else { {t!(tr, "settings.invalid_folder", hint = hint)} }
            }
        }
    }
}

fn set_folder(mut state: AppState, folder: Folder, dir: PathBuf) {
    state.update_settings(|s| match folder {
        Folder::Saves => s.paths.save_dir = Some(dir),
        Folder::Game => s.paths.game_dir = Some(dir),
    });
}
