use std::path::PathBuf;

use dioxus::prelude::*;

use super::pick_folder;
use super::quest_list::phase_text;
use crate::settings::{detect_game_dir, detect_save_dir, is_game_dir, is_save_dir};
use crate::state::{AppState, SyncStatus};

/// First-launch setup, one step at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingStep {
    SaveDir,
    GameDir,
    Sync,
}

/// Full-window setup card for the current step. `on_skip_sync` lets the user continue without
/// quest data.
#[component]
pub fn Onboarding(step: OnboardingStep, on_skip_sync: EventHandler<()>) -> Element {
    let step_no = match step {
        OnboardingStep::SaveDir => 1,
        OnboardingStep::GameDir => 2,
        OnboardingStep::Sync => 3,
    };
    rsx! {
        div { class: "onboarding",
            div { class: "card onboarding-card",
                div { class: "muted small", "Setup · step {step_no} of 3" }
                match step {
                    OnboardingStep::SaveDir => rsx! { FolderStep { key: "{step_no}", saves: true } },
                    OnboardingStep::GameDir => rsx! { FolderStep { key: "{step_no}", saves: false } },
                    OnboardingStep::Sync => rsx! { SyncStep { on_skip: on_skip_sync } },
                }
            }
        }
    }
}

#[component]
fn FolderStep(saves: bool) -> Element {
    let mut state = use_context::<AppState>();
    let detected = use_hook(move || if saves { detect_save_dir() } else { detect_game_dir() });
    let (title, text, picker_title) = if saves {
        (
            "Where are your saves?",
            "The tracker reads your Witcher 3 saves to see which quests you have started, finished or failed, \
             and which playthrough each save belongs to. It only ever reads them.",
            "Witcher 3 save folder",
        )
    } else {
        (
            "Where is the game installed?",
            "The quest list, titles in every language and recommended levels are read from the game's own files. \
             Pick the folder that contains \"content\" and \"bin\".",
            "Witcher 3 game folder",
        )
    };

    let mut accept = move |dir: PathBuf| {
        let valid = if saves { is_save_dir(&dir) } else { is_game_dir(&dir) };
        if !valid {
            state.error.set(Some(format!("{} does not look like the right folder.", dir.display())));
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
        h2 { "{title}" }
        p { "{text}" }
        if let Some(dir) = detected.clone() {
            div { class: "folder-row",
                code { class: "folder-path", "{dir.display()}" }
                button { class: "btn btn-primary", onclick: move |_| accept(dir.clone()), "Use this folder" }
            }
        } else {
            p { class: "muted", "It could not be found automatically." }
        }
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
            "Browse…"
        }
    }
}

#[component]
fn SyncStep(on_skip: EventHandler<()>) -> Element {
    let mut state = use_context::<AppState>();
    let sync = state.sync.read().clone();
    let running = matches!(sync, SyncStatus::Running(_));
    rsx! {
        h2 { "Load the quest list" }
        p {
            "Reads every quest from the game files (a few seconds), then adds cutoff warnings, prerequisites "
            "and missable-quest notes from the Witcher wiki (about a minute)."
        }
        match &sync {
            SyncStatus::Running(phase) => rsx! { p { class: "sync-status", span { class: "spinner" } {phase_text(*phase)} } },
            SyncStatus::Failed(msg) => rsx! { p { class: "sync-status sync-failed", "{msg}" } },
            _ => rsx! {},
        }
        div { class: "modal-actions",
            button { class: "btn btn-ghost", disabled: running, onclick: move |_| on_skip.call(()), "Skip for now" }
            button { class: "btn btn-primary", disabled: running, onclick: move |_| state.start_sync(), "Sync now" }
        }
    }
}
