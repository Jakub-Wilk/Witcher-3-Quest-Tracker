use companion_db::Playthrough;
use dioxus::prelude::*;

use super::{PlaythroughModal, SettingsModal};
use crate::components::ornament::{Divider, Glyph, Icon, Medallion};
use crate::i18n::LANGUAGES;
use crate::settings::Tab;
use crate::state::{AppState, SyncStatus};
use crate::t;

/// Which modal is open.
#[derive(Clone, PartialEq)]
enum Modal {
    Closed,
    New,
    Edit(Playthrough),
    Settings,
}

/// The masthead: medallion and title, the Quests / Workbench tabs, and the app-wide controls
/// (playthrough, save tracking, sync, language, settings).
#[component]
pub fn Header() -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let mut modal = use_signal(|| Modal::Closed);
    let tab = *state.tab.read();
    let syncing = matches!(*state.sync.read(), SyncStatus::Running(_));
    let current = *state.current.read();
    let current_playthrough = state.playthroughs.read().iter().find(|p| Some(p.id) == current).cloned();
    let language = state.settings.read().ui.language.clone();

    rsx! {
        header { class: "masthead",
            div { class: "masthead-row",
                div { class: "brand",
                    Medallion { size: 40, spinning: syncing }
                    div {
                        h1 { class: "brand-title", {t!(tr, "app.title")} }
                        p { class: "brand-tagline", {t!(tr, "app.tagline")} }
                    }
                }
                nav { class: "tabs", "data-active": if tab == Tab::Workbench { "1" } else { "0" },
                    button {
                        class: if tab == Tab::Quests { "tab is-active" } else { "tab" },
                        onclick: move |_| state.set_tab(Tab::Quests),
                        Glyph { icon: Icon::Scroll }
                        span { {t!(tr, "tab.quests")} }
                    }
                    button {
                        class: if tab == Tab::Workbench { "tab is-active" } else { "tab" },
                        onclick: move |_| state.set_tab(Tab::Workbench),
                        Glyph { icon: Icon::Hammer }
                        span { {t!(tr, "tab.workbench")} }
                    }
                    span { class: "tab-ink" }
                }
                div { class: "head-controls",
                    if !state.playthroughs.read().is_empty() {
                        div { class: "run-picker",
                            Glyph { icon: Icon::Sword, class: "run-picker-glyph" }
                            select {
                                class: "select run-select",
                                title: t!(tr, "header.playthrough"),
                                onchange: move |e| {
                                    if let Ok(id) = e.value().parse() {
                                        state.select_playthrough(Some(id));
                                    }
                                },
                                for p in state.playthroughs.read().iter() {
                                    option { value: "{p.id}", selected: current == Some(p.id),
                                        "{p.name}"
                                        if p.is_new_game_plus { " · NG+" }
                                    }
                                }
                            }
                            if let Some(p) = current_playthrough {
                                button {
                                    class: "btn-icon",
                                    title: t!(tr, "header.edit_playthrough"),
                                    onclick: move |_| modal.set(Modal::Edit(p.clone())),
                                    Glyph { icon: Icon::Pencil }
                                }
                            }
                        }
                    }
                    button {
                        class: "btn-icon",
                        title: t!(tr, "header.new_playthrough"),
                        onclick: move |_| modal.set(Modal::New),
                        Glyph { icon: Icon::Plus }
                    }
                    SaveChip {}
                    button {
                        class: if syncing { "btn-icon is-busy" } else { "btn-icon" },
                        title: t!(tr, "header.sync"),
                        disabled: syncing,
                        onclick: move |_| state.start_sync(),
                        Glyph { icon: Icon::Sync }
                    }
                    label { class: "lang-select", title: t!(tr, "header.language"),
                        select {
                            class: "select",
                            onchange: move |e| state.set_language(e.value()),
                            for lang in LANGUAGES {
                                option { value: lang.code, selected: lang.code == language, "{lang.native}" }
                            }
                        }
                    }
                    button {
                        class: "btn-icon",
                        title: t!(tr, "header.settings"),
                        onclick: move |_| modal.set(Modal::Settings),
                        Glyph { icon: Icon::Gear }
                    }
                }
            }
            Divider { class: "masthead-divider" }
        }
        match modal() {
            Modal::Closed => rsx! {},
            Modal::New => rsx! { PlaythroughModal { on_close: move |_| modal.set(Modal::Closed) } },
            Modal::Edit(p) => rsx! {
                PlaythroughModal { existing: p, on_close: move |_| modal.set(Modal::Closed) }
            },
            Modal::Settings => rsx! { SettingsModal { on_close: move |_| modal.set(Modal::Closed) } },
        }
    }
}

/// Save tracking status: a pulsing dot while the save folder is watched, the last save read,
/// and a button to read the whole folder again.
#[component]
fn SaveChip() -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let tracker = state.tracker.read().clone();
    let settings = state.settings.read();
    let watching = settings.tracking.watch_saves && settings.valid_save_dir().is_some();
    drop(settings);
    let linked = state
        .playthroughs
        .read()
        .iter()
        .any(|p| Some(p.id) == *state.current.read() && p.link.is_some());

    rsx! {
        div {
            class: if watching { "save-chip is-watching" } else { "save-chip" },
            title: if watching { t!(tr, "save.watching_hint") } else { t!(tr, "save.not_watching_hint") },
            if tracker.scanning {
                span { class: "save-spinner" }
                span { {t!(tr, "save.reading")} }
            } else {
                span { class: "save-dot" }
                if let Some(last) = &tracker.last {
                    span { class: "save-last", "{last}" }
                } else if watching {
                    span { if linked { {t!(tr, "save.watching_linked")} } else { {t!(tr, "save.watching")} } }
                } else {
                    span { {t!(tr, "save.not_watching")} }
                }
            }
            button {
                class: "save-rescan",
                title: t!(tr, "save.rescan"),
                disabled: tracker.scanning,
                onclick: move |_| state.rescan_saves(false),
                Glyph { icon: Icon::Sync }
            }
        }
    }
}
