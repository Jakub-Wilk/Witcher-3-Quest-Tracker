use std::path::PathBuf;
use std::rc::Rc;

use dioxus::prelude::*;

use crate::save_tracker;
use crate::state::AppState;

/// Invisible: reads the save folder once, then watches it while mounted. Remount it (via
/// `key`) to watch a different folder; unmounting stops the watcher.
#[component]
pub fn SaveWatcher(dir: String, watch: bool) -> Element {
    let mut state = use_context::<AppState>();
    // Read the whole folder once per mount (after render, so no signal is written during it).
    use_effect(move || state.rescan_saves(false));
    use_hook(move || {
        if !watch {
            return None;
        }
        match save_tracker::watch(&PathBuf::from(&dir)) {
            Ok((watcher, mut rx)) => {
                spawn(async move {
                    while let Some(batch) = save_tracker::next_batch(&mut rx).await {
                        for path in batch {
                            match save_tracker::read_when_complete(path.clone()).await {
                                Ok(save) => state.handle_save(save),
                                Err(e) => tracing::warn!("Could not read {}: {e}", path.display()),
                            }
                        }
                    }
                });
                Some(Rc::new(watcher))
            }
            Err(e) => {
                state.error.set(Some(format!("Could not watch the save folder: {e}")));
                None
            }
        }
    });
    rsx! {}
}
