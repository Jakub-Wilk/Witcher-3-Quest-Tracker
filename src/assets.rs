//! Files the webview loads from the app itself: the bundled fonts and the item pictures stored in
//! the database. Nothing is loaded from the network.

use dioxus::desktop::wry::http::{Response, StatusCode};
use dioxus::desktop::{AssetRequest, RequestAsyncResponder, use_asset_handler};
use dioxus::prelude::*;

use crate::state::AppState;

/// Inlined rather than loaded via `asset!` so a plain `cargo run` works without the `dx` CLI.
pub const STYLE: &str = concat!(
    include_str!("../assets/css/fonts.css"),
    include_str!("../assets/css/theme.css"),
    include_str!("../assets/css/shell.css"),
    include_str!("../assets/css/quests.css"),
    include_str!("../assets/css/workbench.css"),
);

macro_rules! fonts {
    ($($file:literal),* $(,)?) => {
        const FONTS: &[(&str, &[u8])] = &[$(($file, include_bytes!(concat!("../assets/fonts/", $file)))),*];
    };
}

fonts![
    "cinzel-latin-500-normal.woff2",
    "cinzel-latin-700-normal.woff2",
    "cinzel-latin-900-normal.woff2",
    "cinzel-latin-ext-500-normal.woff2",
    "cinzel-latin-ext-700-normal.woff2",
    "cinzel-latin-ext-900-normal.woff2",
    "im-fell-english-latin-400-normal.woff2",
    "im-fell-english-latin-400-italic.woff2",
    "eb-garamond-latin-400-normal.woff2",
    "eb-garamond-latin-600-normal.woff2",
    "eb-garamond-latin-700-normal.woff2",
    "eb-garamond-latin-400-italic.woff2",
    "eb-garamond-latin-ext-400-normal.woff2",
    "eb-garamond-latin-ext-600-normal.woff2",
    "eb-garamond-latin-ext-700-normal.woff2",
    "eb-garamond-latin-ext-400-italic.woff2",
    "eb-garamond-cyrillic-400-normal.woff2",
    "eb-garamond-cyrillic-600-normal.woff2",
    "eb-garamond-cyrillic-700-normal.woff2",
    "eb-garamond-cyrillic-400-italic.woff2",
    "amiri-arabic-400-normal.woff2",
    "amiri-arabic-700-normal.woff2",
];

/// Serves `/fonts/<file>` and `/icons/<item id>` while the calling component is mounted.
pub fn use_asset_routes(state: AppState) {
    use_asset_handler("fonts", |request: AssetRequest, responder: RequestAsyncResponder| {
        let file = last_segment(&request);
        match FONTS.iter().find(|(name, _)| *name == file) {
            Some((_, bytes)) => responder.respond(ok("font/woff2", "max-age=31536000", bytes.to_vec())),
            None => responder.respond(not_found()),
        }
    });
    use_asset_handler("icons", move |request: AssetRequest, responder: RequestAsyncResponder| {
        let id = last_segment(&request);
        let image = companion_db::items::get_image(&state.db.read(), &id);
        match image {
            // Short-lived: a sync may replace a picture.
            Ok(Some(image)) => responder.respond(ok(&image.content_type, "max-age=600", image.image)),
            Ok(None) => responder.respond(not_found()),
            Err(e) => {
                tracing::warn!("Could not read the picture of {id}: {e}");
                responder.respond(not_found());
            }
        }
    });
}

fn last_segment(request: &AssetRequest) -> String {
    let segment = request.uri().path().rsplit('/').next().unwrap_or_default();
    percent_decode(segment)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], bytes.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn ok(content_type: &str, cache: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .header("Content-Type", content_type)
        .header("Cache-Control", cache)
        .body(body)
        .expect("valid response")
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()).expect("valid response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("swallow-2"), "swallow-2");
        assert_eq!(percent_decode("a%20b%C3%A9"), "a bé");
        assert_eq!(percent_decode("bad%zz"), "bad%zz");
    }
}
