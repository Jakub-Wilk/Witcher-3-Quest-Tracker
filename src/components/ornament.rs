//! Decorative SVG: the gilded divider, the witcher medallion and the line icons.

use dioxus::prelude::*;

/// Gilded divider with a central diamond, drawn in `currentColor`.
#[component]
pub fn Divider(#[props(default)] class: String) -> Element {
    rsx! {
        svg {
            class: "divider {class}",
            view_box: "0 0 400 16",
            preserve_aspect_ratio: "none",
            "aria-hidden": "true",
            path { d: "M0 8 H170 M230 8 H400", stroke: "currentColor", stroke_width: "1" }
            path { d: "M150 8 L170 4 L170 12 Z M250 8 L230 4 L230 12 Z", fill: "currentColor", opacity: ".6" }
            path { d: "M200 1 L209 8 L200 15 L191 8 Z", fill: "none", stroke: "currentColor", stroke_width: "1.2" }
            circle { cx: "200", cy: "8", r: "2", fill: "currentColor" }
        }
    }
}

/// A witcher medallion: rings, runic ticks and the Aard sign. `spinning` turns it into a
/// loading indicator; otherwise it hums now and then.
#[component]
pub fn Medallion(#[props(default = 56)] size: u32, #[props(default)] spinning: bool) -> Element {
    let class = if spinning { "medallion is-spinning" } else { "medallion is-idle" };
    rsx! {
        svg {
            class,
            width: "{size}",
            height: "{size}",
            view_box: "0 0 64 64",
            "aria-hidden": "true",
            defs {
                radialGradient { id: "med-metal", cx: "40%", cy: "35%", r: "70%",
                    stop { offset: "0", stop_color: "#d9c08a" }
                    stop { offset: ".55", stop_color: "#8d7445" }
                    stop { offset: "1", stop_color: "#3d3120" }
                }
            }
            circle { cx: "32", cy: "32", r: "30", fill: "url(#med-metal)", stroke: "#1a140c", stroke_width: "1.5" }
            circle { cx: "32", cy: "32", r: "24", fill: "#1d1710", stroke: "#c9a45c", stroke_width: "1" }
            for a in (0..24).map(|i| i * 15) {
                line {
                    key: "{a}",
                    x1: "32",
                    y1: "3.5",
                    x2: "32",
                    y2: if a % 45 == 0 { "8" } else { "6" },
                    stroke: "#2a2014",
                    stroke_width: "1.2",
                    transform: "rotate({a} 32 32)",
                }
            }
            g {
                class: "medallion-sign",
                stroke: "#e6c983",
                stroke_width: "2",
                fill: "none",
                stroke_linejoin: "round",
                path { d: "M32 17 L46 42 L18 42 Z" }
                path { d: "M32 26 V42 M25 36 L32 30 L39 36", stroke_width: "1.5" }
            }
        }
    }
}

/// Line icons, 24×24, stroked in `currentColor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Search,
    Sync,
    Gear,
    Pencil,
    Plus,
    Minus,
    Close,
    Check,
    Hammer,
    Scroll,
    Warning,
    Pin,
    Quill,
    External,
    Chevron,
    Sword,
    Flask,
    Rune,
    Bag,
    Save,
}

impl Icon {
    fn paths(self) -> &'static [&'static str] {
        match self {
            Icon::Search => &["M10.5 4a6.5 6.5 0 1 1 0 13 6.5 6.5 0 0 1 0-13z", "M15.5 15.5 20 20"],
            Icon::Sync => &["M19 8a8 8 0 0 0-14.3 1", "M5 16a8 8 0 0 0 14.3-1", "M19 3v5h-5", "M5 21v-5h5"],
            Icon::Gear => &[
                "M12 8.5a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7z",
                "M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1",
            ],
            Icon::Pencil => &["M4 20l1-4L16 5l3 3L8 19z", "M14 7l3 3"],
            Icon::Plus => &["M12 5v14M5 12h14"],
            Icon::Minus => &["M5 12h14"],
            Icon::Close => &["M6 6l12 12M18 6 6 18"],
            Icon::Check => &["M5 12.5l4.5 4.5L19 7.5"],
            Icon::Hammer => &["M14 4l6 6-2.5 2.5-6-6z", "M11.5 6.5l-8 8 3 3 8-8", "M15.5 3.5l1-1"],
            Icon::Scroll => &["M7 4h11a2 2 0 0 1 0 4H7", "M7 4a2 2 0 0 0 0 4v10a2 2 0 0 0 2 2h9", "M18 8v10a2 2 0 0 1-4 0", "M10 11h5M10 14h4"],
            Icon::Warning => &["M12 3.5 21.5 20h-19z", "M12 10v4.5M12 17v.5"],
            Icon::Pin => &["M9 3h6l-1 6 3 3H7l3-3z", "M12 12v9"],
            Icon::Quill => &["M20 4C12 5 7 10 5 19", "M20 4c-1 6-5 10-11 11", "M4 20l2-2"],
            Icon::External => &["M14 4h6v6", "M20 4l-9 9", "M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"],
            Icon::Chevron => &["M7 10l5 5 5-5"],
            Icon::Sword => &["M19.5 4.5 9 15", "M19.5 4.5v4l-9 9M19.5 4.5h-4l-9 9", "M6 15l3 3M4 20l3-3"],
            Icon::Flask => &["M9.5 3h5M10 3v6L4.5 18.5A1.7 1.7 0 0 0 6 21h12a1.7 1.7 0 0 0 1.5-2.5L14 9V3", "M7 15h10"],
            Icon::Rune => &["M12 3l7 9-7 9-7-9z", "M12 7v10M9 10l3 2 3-2"],
            Icon::Bag => &["M6 8h12l-1 12H7z", "M9 8V6a3 3 0 0 1 6 0v2"],
            Icon::Save => &["M5 4h11l3 3v12a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z", "M8 4v5h7V4", "M8 20v-6h8v6"],
        }
    }
}

#[component]
pub fn Glyph(icon: Icon, #[props(default)] class: String) -> Element {
    rsx! {
        svg {
            class: "icon {class}",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.7",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "aria-hidden": "true",
            for d in icon.paths() {
                path { d: "{d}" }
            }
        }
    }
}
