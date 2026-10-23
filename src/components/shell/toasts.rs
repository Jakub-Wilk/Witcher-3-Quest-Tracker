use dioxus::prelude::*;

use crate::components::ornament::{Glyph, Icon};
use crate::state::AppState;
use crate::t;

/// The informational and error toasts.
#[component]
pub fn Toasts() -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let notice = state.notice.read().clone();
    let error = state.error.read().clone();
    rsx! {
        div { class: "toasts",
            if let Some(notice) = notice {
                div { key: "notice-{notice}", class: "toast toast-info",
                    Glyph { icon: Icon::Scroll }
                    span { "{notice}" }
                    button { class: "btn-x", title: t!(tr, "common.dismiss"), onclick: move |_| state.notice.set(None),
                        Glyph { icon: Icon::Close }
                    }
                }
            }
            if let Some(err) = error {
                div { key: "error-{err}", class: "toast toast-error",
                    Glyph { icon: Icon::Warning }
                    span { "{err}" }
                    button { class: "btn-x", title: t!(tr, "common.dismiss"), onclick: move |_| state.error.set(None),
                        Glyph { icon: Icon::Close }
                    }
                }
            }
        }
    }
}
