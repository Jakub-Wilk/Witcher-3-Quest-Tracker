use dioxus::prelude::*;

use crate::state::AppState;

/// An item's picture in a frame tinted by its rarity; the initial when there is no picture.
#[component]
pub fn ItemIcon(item_id: String, #[props(default = 40)] size: u32) -> Element {
    let state = use_context::<AppState>();
    let catalog = state.catalog.read();
    let item = catalog.items.get(&item_id);
    let quality = item.map_or(1, |i| i.quality);
    let has_image = item.is_some_and(|i| i.has_image);
    let initial = item.and_then(|i| i.name.chars().next()).unwrap_or('?');
    rsx! {
        span { class: "item-icon tier-q{quality}", style: "width: {size}px; height: {size}px",
            if has_image {
                img { src: "/icons/{item_id}", alt: "", loading: "lazy", draggable: "false" }
            } else {
                span { class: "item-icon-fallback", "{initial}" }
            }
        }
    }
}
