fn main() {
    // Embeds the icon shown for the .exe in Explorer, the taskbar and shortcuts. The window's
    // title bar icon is set separately at runtime from assets/icon.png.
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile().expect("failed to embed Windows icon resource");
    }
}
