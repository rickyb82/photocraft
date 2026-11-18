//! PhotoCraft in the browser.
//!
//! Runs the same [`photocraft_ui_egui::PhotocraftApp`] as the desktop app through eframe's web
//! runner (wgpu: WebGPU where available, WebGL2 otherwise). Build with `trunk build --release`
//! from this directory; see `docs/development.md` ("Web build").
//!
//! Differences from the desktop app:
//! - no TCP control server (browsers can't listen on sockets);
//! - file pickers return browser file contents asynchronously through `Services::file_dialog`;
//!   this includes File → Open and commands such as Place Embedded;
//! - saving/exporting triggers a browser download (or goes to a page that hosts the app, below);
//! - dropped files are read asynchronously by `web::WebShell` and delivered through `Services::inbox`.
//!
//! URL query flags, each a whole parameter of the query (`query`), so a name a page puts beside
//! them switches none on: `?cpu` forces the CPU canvas path (same as `PHOTOCRAFT_CPU_CANVAS=1`);
//! `?webgl` forces the WebGL2 backend instead of WebGPU; `?free-mb=N` pretends only N MiB can be
//! allocated, so the memory gates and the low-memory warning can be seen without gigabytes of
//! images (diagnostics only).
//!
//! `?lang=<code>` (`it`, `pt-BR`, ...) shows the interface in that language when the build has its
//! catalog, and is ignored otherwise. It is kept as the Preferences › Interface choice, which is
//! where the interface reads its language from; without it the browser build starts in English.
//!
//! `?host=parent` (inside an `<iframe>`) talks to a same-origin parent page instead of
//! downloading: the page sends the file to open and keeps the documents and pictures PhotoCraft
//! saves (`host`). A page can also host PhotoCraft in its own document (no iframe) with
//! `data-host="page"` on the canvas, talking over DOM `CustomEvent`s on the canvas.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(any(target_arch = "wasm32", test))]
mod diagnostics;

#[cfg(target_arch = "wasm32")]
mod host;

#[cfg(any(target_arch = "wasm32", test))]
mod host_files;

#[cfg(any(target_arch = "wasm32", test))]
mod preset_bridge;

#[cfg(any(target_arch = "wasm32", test))]
mod query;

#[cfg(target_arch = "wasm32")]
mod indexed_presets;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("photocraft-web only runs in the browser: build it with `trunk build --release` in apps/photocraft-web");
}

#[cfg(test)]
mod tests {
    /// A WebAssembly module that never downloads (or doesn't match the page) runs no Rust, so the
    /// page itself replaces the endless "Loading" with what may have gone wrong and a way to
    /// retry, without a hand-written script.
    #[test]
    fn loading_page_offers_a_reload_when_startup_stalls() {
        let html = include_str!("../index.html");
        let (_, hint) = html.split_once("<p id=\"photocraft_stalled\">").expect("stalled-startup hint in the loading element");
        assert!(hint.contains("failed to") && hint.contains("<a href=\"\">Reload</a>"), "{hint}");
        assert!(html.contains("animation: photocraft-stalled"), "the hint appears after a delay");
        assert!(!html.contains("<script"), "trunk injects the generated loader; the page has no script of its own");
    }

    /// A module that traps (an allocation it could not satisfy, or a panic) leaves the last frame
    /// on screen. The page already holds the message, hidden, with a way to reload; Rust only
    /// reveals it and fills in the detail line, so it works even when Rust can no longer build text.
    #[test]
    fn the_page_holds_a_hidden_stopped_message_with_a_reload_link() {
        let html = include_str!("../index.html");
        let (_, element) = html.split_once("<div id=\"photocraft_stopped\"").expect("a stopped-message element");
        let (element, _) = element.split_once("</div>").expect("the element is closed");
        assert!(element.trim_start().starts_with("hidden"), "hidden until a trap: {element}");
        assert!(element.contains("role=\"alert\""), "{element}");
        assert!(element.contains("<a href=\"\">Reload</a>"), "{element}");
        assert!(element.contains("can't be recovered"), "says what is lost: {element}");
        assert!(element.contains("id=\"photocraft_stopped_detail\""), "a line for the cause: {element}");
        assert!(html.contains("#photocraft_stopped[hidden]"), "display: flex must not defeat the hidden attribute");
        assert!(html.contains("pointer-events: auto"), "the link works over the canvas");
        assert!(!html.contains("<script"), "still no script of its own");
    }
}
