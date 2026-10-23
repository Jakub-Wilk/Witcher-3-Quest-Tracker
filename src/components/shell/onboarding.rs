use std::path::PathBuf;

use dioxus::prelude::*;

use super::phase_text;
use crate::components::ornament::{Divider, Medallion};
use crate::components::pick_folder;
use crate::settings::{detect_game_dir, detect_save_dir, is_game_dir, is_save_dir};
use crate::state::{AppState, SyncStatus};
use crate::t;

/// First-launch setup, one step at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingStep {
    SaveDir,
    GameDir,
    Sync,
}

/// Full-window setup card for the current step. `on_skip_sync` lets the user continue without
/// game data.
#[component]
pub fn Onboarding(step: OnboardingStep, on_skip_sync: EventHandler<()>) -> Element {
    let state = use_context::<AppState>();
    let tr = *state.tr.read();
    let step_no = match step {
        OnboardingStep::SaveDir => 1,
        OnboardingStep::GameDir => 2,
        OnboardingStep::Sync => 3,
    };
    let syncing = matches!(*state.sync.read(), SyncStatus::Running(_));
    rsx! {
        div { class: "onboarding",
            div { class: "panel onboarding-card", key: "{step_no}",
                div { class: "onboarding-head",
                    Medallion { size: 72, spinning: syncing }
                    h1 { class: "brand-title", {t!(tr, "app.title")} }
                    p { class: "brand-tagline", {t!(tr, "app.tagline")} }
                    Divider {}
                }
                ol { class: "steps",
                    for n in 1..=3 {
                        li { class: if n < step_no { "step is-done" } else if n == step_no { "step is-current" } else { "step" } }
                    }
                }
                div { class: "kicker", {t!(tr, "setup.step", n = step_no, total = 3)} }
                match step {
                    OnboardingStep::SaveDir => rsx! { FolderStep { saves: true } },
                    OnboardingStep::GameDir => rsx! { FolderStep { saves: false } },
                    OnboardingStep::Sync => rsx! { SyncStep { on_skip: on_skip_sync } },
                }
            }
        }
    }
}

#[component]
fn FolderStep(saves: bool) -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let detected = use_hook(move || if saves { detect_save_dir() } else { detect_game_dir() });
    let (title, text, picker_title) = if saves {
        (t!(tr, "setup.saves_title"), t!(tr, "setup.saves_text"), t!(tr, "setup.saves_picker"))
    } else {
        (t!(tr, "setup.game_title"), t!(tr, "setup.game_text"), t!(tr, "setup.game_picker"))
    };

    let mut accept = move |dir: PathBuf| {
        let valid = if saves { is_save_dir(&dir) } else { is_game_dir(&dir) };
        if !valid {
            state.error.set(Some(t!(tr, "setup.wrong_folder", path = dir.display())));
            return;
        }
        state.update_settings(|s| {
            if saves {
                s.paths.save_dir = Some(dir);
            } else {
                s.paths.game_dir = Some(dir);
            }
        });
    };

    rsx! {
        h2 { class: "onboarding-title", "{title}" }
        p { "{text}" }
        if let Some(dir) = detected.clone() {
            div { class: "folder-row",
                code { class: "folder-path", "{dir.display()}" }
                button { class: "btn btn-gold", onclick: move |_| accept(dir.clone()), {t!(tr, "setup.use_folder")} }
            }
        } else {
            p { class: "muted", {t!(tr, "setup.not_found")} }
        }
        div { class: "onboarding-actions",
            button {
                class: "btn btn-ghost",
                onclick: move |_| {
                    let start = detected.clone();
                    spawn(async move {
                        if let Some(dir) = pick_folder(picker_title, start).await {
                            accept(dir);
                        }
                    });
                },
                {t!(tr, "common.browse")}
            }
        }
    }
}

#[component]
fn SyncStep(on_skip: EventHandler<()>) -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let sync = state.sync.read().clone();
    let running = matches!(sync, SyncStatus::Running(_));
    rsx! {
        h2 { class: "onboarding-title", {t!(tr, "setup.sync_title")} }
        p { {t!(tr, "setup.sync_text")} }
        match &sync {
            SyncStatus::Running(phase) => rsx! { p { class: "sync-line", span { class: "sync-banner-spinner" } {phase_text(tr, *phase)} } },
            SyncStatus::Failed(msg) => rsx! { p { class: "sync-line is-failed", "{msg}" } },
            _ => rsx! {},
        }
        div { class: "onboarding-actions",
            button { class: "btn btn-ghost", disabled: running, onclick: move |_| on_skip.call(()), {t!(tr, "setup.skip")} }
            button { class: "btn btn-gold", disabled: running, onclick: move |_| state.start_sync(), {t!(tr, "setup.sync_now")} }
        }
    }
}
