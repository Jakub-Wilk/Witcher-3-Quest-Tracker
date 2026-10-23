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

/// Outline of the wolf's head, shared by the medallion and its bevel.
const WOLF_HEAD: &str = "M32 13.5 L25 10 L15 1.5 L13.5 16 L8 21 L10.5 27 L4.5 31.5 L10 36 L7.5 42.5 L15 43 L18 50 L24 49 L32 62.5 L40 49 L46 50 L49 43 L56.5 42.5 L54 36 L59.5 31.5 L53.5 27 L56 21 L50.5 16 L49 1.5 L39 10 Z";

/// The School of the Wolf medallion (the same art as `assets/logo.svg`). It hums now and then;
/// `spinning` makes it quiver with glowing eyes, as it does near magic, while work is running.
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
                radialGradient { id: "wolf-metal", cx: "45%", cy: "28%", r: "78%",
                    stop { offset: "0", stop_color: "#f3dca2" }
                    stop { offset: ".4", stop_color: "#bf9d5e" }
                    stop { offset: ".8", stop_color: "#6e5530" }
                    stop { offset: "1", stop_color: "#3f311c" }
                }
                linearGradient { id: "wolf-snout", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "#f6e3b0" }
                    stop { offset: "1", stop_color: "#a5874e" }
                }
                radialGradient { id: "wolf-eye", cx: "50%", cy: "50%", r: "60%",
                    stop { offset: "0", stop_color: "#fff3c8" }
                    stop { offset: "1", stop_color: "#e0703a" }
                }
            }
            circle { cx: "32", cy: "6.5", r: "3.4", fill: "none", stroke: "#1a140c", stroke_width: "3" }
            circle { cx: "32", cy: "6.5", r: "3.4", fill: "none", stroke: "#c9a45c", stroke_width: "1.5" }
            path { d: WOLF_HEAD, fill: "url(#wolf-metal)", stroke: "#1a140c", stroke_width: "1.4", stroke_linejoin: "round" }
            path {
                d: WOLF_HEAD,
                fill: "none",
                stroke: "#f6e3b0",
                stroke_opacity: ".45",
                stroke_width: ".9",
                stroke_linejoin: "round",
                transform: "translate(32 32) scale(.88) translate(-32 -32)",
            }
            path { d: "M16.5 6 L18.3 15 L23.5 11.3 Z M47.5 6 L45.7 15 L40.5 11.3 Z", fill: "#2a2014", fill_opacity: ".75" }
            g { fill: "none", stroke: "#2a2014", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M11 23.5 L17 27 M9 31.5 L16 32.5 M11.5 39.5 L17.5 38 M53 23.5 L47 27 M55 31.5 L48 32.5 M52.5 39.5 L46.5 38", stroke_width: "1.2" }
                path { d: "M32 15 V20.5 M27.5 14.5 L30 19.5 M36.5 14.5 L34 19.5", stroke_width: "1" }
            }
            path { d: "M19 19.5 L29.5 24.5 L27.5 26.3 Z M45 19.5 L34.5 24.5 L36.5 26.3 Z", fill: "#1a140c" }
            path { d: "M22.5 40.5 L28.5 41 L32 44 L35.5 41 L41.5 40.5 L38 50 Q32 55.5 26 50 Z", fill: "#1a140c" }
            path { d: "M24.4 41.2 L26.2 47 L27.8 41.6 Z M39.6 41.2 L37.8 47 L36.2 41.6 Z", fill: "#f1e2bf" }
            path { d: "M27.3 51.2 L28.6 46.8 L30 51.9 Z M36.7 51.2 L35.4 46.8 L34 51.9 Z", fill: "#d9c7a0" }
            path {
                d: "M32 21.5 L37.5 27.5 L38.5 36 L35.3 41.3 L28.7 41.3 L25.5 36 L26.5 27.5 Z",
                fill: "url(#wolf-snout)",
                stroke: "#1a140c",
                stroke_width: "1",
                stroke_linejoin: "round",
            }
            path { d: "M32 24.5 V34", stroke: "#8a6c35", stroke_width: ".9", stroke_linecap: "round" }
            path { d: "M28.6 37.2 Q32 35.8 35.4 37.2 L32 41.2 Z", fill: "#1a140c" }
            g { class: "medallion-sign", fill: "url(#wolf-eye)", stroke: "#1a140c", stroke_width: ".6",
                path { d: "M21 24.2 L27.6 26.6 L23 28.4 Z" }
                path { d: "M43 24.2 L36.4 26.6 L41 28.4 Z" }
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
