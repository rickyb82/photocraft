//! Host modes: a page that runs PhotoCraft (as an `<iframe>`, or in its own document) opens
//! files in it and keeps what it saves instead of the browser downloading. The messages and the
//! two transports are `craft-web-host`'s (storytold/craft-libs), the same contract as the other
//! Craft apps' bridges, with the `photocraft:` prefix. This module is PhotoCraft's part of it.
//!
//! - `?host=parent` (an `<iframe>`, such as a file server's own page) talks to the same-origin
//!   parent over `postMessage`; `data-host="page"` on the canvas (the page's own document, no
//!   iframe) uses DOM `CustomEvent`s on the canvas instead. `?host=parent` wins when both are
//!   present. With no parent, or a parent on another origin, there is no bridge: the normal app
//!   with downloads.
//! - `photocraft:ready` once the app is running and listening; send a file now. PhotoCraft reads
//!   bytes, so it says `file: false` and a host sends `bytes`.
//! - `photocraft:open` (from the host): `{ name, bytes }`, bytes an `ArrayBuffer` or `Uint8Array`,
//!   name a bare file name. It opens like a file picked with File › Open, in its own tab. The
//!   name is also the document's file when File › Save can write the file back whole (a PSD, PSB,
//!   .pcraft or OpenRaster document, or a flat picture such as a PNG or JPEG that holds one
//!   image), so Save hands it back under the same name and format. Any other file (a camera raw,
//!   an animation, a file without a name) opens without a file, and its first Save is a new one.
//!   A file sent as a `File`, one over [`MAX_OPEN_BYTES`], and one the browser has no memory for
//!   are refused before they are copied, with `photocraft:failed`.
//! - `photocraft:save` (to the host): `{ name, saveAs, bytes }` for every document or picture
//!   PhotoCraft would otherwise download: Save, Save As, Save a Copy, Export, Save for Web's
//!   pictures (which files those are: [`host_files::goes_to_page`]). The host stores the file and
//!   reports failures itself; PhotoCraft counts a document as saved once it is handed over. Save
//!   writes the opened file under its own name and format where it can; where it can't (layers
//!   added to a PNG), and for a new document, the name is a new one (`photo.psd`). PhotoCraft
//!   doesn't tell the host which saves make a file the document's, so `saveAs` is always `false`.
//!   Files that are not documents (presets, actions, swatches, Camera Raw settings, shortcut
//!   sets, preferences, logs, the HTML page of Save for Web) are still downloaded: a page that
//!   keeps documents has no place for them.
//! - `photocraft:dirty` whenever unsaved changes appear or go away, `photocraft:failed` when the
//!   app couldn't start or refused a file.
//!
//! In both host modes the app starts on its Home screen, saves are handed to the host rather than
//! downloaded, and the host guards leaving the page.

use std::cell::RefCell;
use std::rc::Rc;

use craft_web_host::web::{FileData, Host, Message};
use photocraft_ui_egui::{PhotocraftApp, Services, i18n};

use crate::host_files::{self, MAX_OPEN_BYTES};

/// The app id in the host messages (`photocraft:ready`).
const APP: &str = "photocraft";

/// A file the host sent, under the name it sent, waiting for the next logic pass: its bytes, or
/// why it was refused.
type Arrived = (String, Result<Vec<u8>, String>);

#[derive(Clone)]
pub struct Bridge {
    host: Rc<Host>,
    arrived: Rc<RefCell<Vec<Arrived>>>,
}

impl Bridge {
    /// The bridge for this page, or `None` when PhotoCraft isn't hosted: no `?host=parent` and no
    /// `data-host="page"` on the canvas, an unframed `?host=parent` page (it stays the normal app
    /// with downloads), or a parent on another origin.
    pub fn detect(canvas: &web_sys::HtmlCanvasElement) -> Option<Self> {
        let host = Host::connect(APP, canvas)?;
        // An opaque origin (a file:// page in some browsers) can't be a postMessage target.
        if host.mode() == craft_web_host::Mode::Parent && web_sys::window()?.location().origin().ok()? == "null" {
            return None;
        }
        Some(Self { host: Rc::new(host), arrived: Rc::default() })
    }

    /// Hear the files the host sends, and hand it the documents and pictures PhotoCraft saves
    /// instead of downloading them.
    pub fn connect(&self, services: &mut Services, ctx: egui::Context) {
        self.listen(ctx);
        let host = self.host.clone();
        // Whatever isn't a document or a picture keeps going where it went: a download.
        let mut download = services.write.take();
        services.write = Some(Box::new(move |path: &str, bytes: &[u8]| {
            let name = host_files::file_name(path);
            if host_files::goes_to_page(name) {
                return host.post_save(name, bytes, None, false);
            }
            match download.as_mut() {
                Some(download) => download(path, bytes),
                None => Err("no writer configured".into()),
            }
        }));
        // A file handed to the host is saved as far as PhotoCraft can know: the host stores it and
        // reports a failure itself, as the contract has it for a save sent without an id. A
        // download, which this flag is set for, may be cancelled and reports nothing back.
        services.write_unconfirmed = false;
    }

    pub fn post_ready(&self) {
        let _ = self.host.post_ready(env!("CARGO_PKG_VERSION"), false);
    }

    /// The app couldn't start (no WebGPU or WebGL2, for example): let the host say so.
    pub fn post_failed(&self, error: &str) {
        let _ = self.host.post_failed(error, None);
    }

    /// Before each logic pass: open the files the host sent since the last one, and show the ones
    /// that were refused where a failed open is shown.
    pub fn open_files(&self, app: &mut PhotocraftApp) {
        let arrived = std::mem::take(&mut *self.arrived.borrow_mut());
        if arrived.is_empty() {
            return;
        }
        // A file can arrive before the first frame, which otherwise applies the interface
        // language: its status line and notices would come out in English.
        i18n::set_current(i18n::Lang::from_pref(&app.session.prefs().interface.language));
        for (name, file) in arrived {
            if let Err(why) = file.and_then(|bytes| app.open_from_host(&name, &bytes).map(drop)) {
                app.open_failed(&name, &why);
            }
        }
    }

    /// After each pass: tell the host when unsaved changes appear or go away (the crate does not
    /// repeat a state it already reported).
    pub fn report_dirty(&self, app: &PhotocraftApp) {
        self.host.post_dirty(app.session.documents().iter().any(|document| document.is_dirty()));
    }

    /// Hear `photocraft:open` and queue the file for the next logic pass: opening needs the app,
    /// which the runner owns.
    fn listen(&self, ctx: egui::Context) {
        let (host, arrived) = (self.host.clone(), self.arrived.clone());
        self.host.listen(move |message| {
            // What else a host may say (what it supports, a PDF page to attach, a save or a
            // hand-off it finished) asks nothing of PhotoCraft.
            let Message::Open { name, data } = message else { return };
            let file = take(&data, &arrived.borrow());
            if let Err(why) = &file {
                log::warn!("host file {name} was not opened: {why}");
                let _ = host.post_failed(&format!("Couldn't open {name}: {why}"), None);
            }
            arrived.borrow_mut().push((name, file));
            ctx.request_repaint();
        });
    }
}

/// The bytes of a file the host sent, copied into the app, or why it is refused. A host's file is
/// untrusted input, so everything is decided from the data's own length before the copy: a file
/// over the limit, one that would wait behind more than the limit, and one the browser has no
/// memory for (on the web an allocation the browser refuses stops the whole app, with every open
/// document). The crate has cleaned the name and cut it to 255 characters.
fn take(data: &FileData, waiting: &[Arrived]) -> Result<Vec<u8>, String> {
    let FileData::Bytes(array) = data else {
        return Err(format!("it came as a File, and PhotoCraft reads bytes (at most {} MB of them)", MAX_OPEN_BYTES >> 20));
    };
    let size = u64::from(array.length());
    let waiting = waiting.iter().filter_map(|(_, file)| file.as_ref().ok()).fold(0u64, |held, bytes| held.saturating_add(bytes.len() as u64));
    if let Some(why) = host_files::open_refusal(size, waiting) {
        return Err(why);
    }
    photocraft_raster::headroom::ensure(size).map_err(|_| host_files::no_memory(size))?;
    Ok(array.to_vec())
}
