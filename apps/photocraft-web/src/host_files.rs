//! What the host bridge (`host.rs`) decides about files without asking the browser, so native
//! tests cover it: which of the files PhotoCraft writes go to the page that hosts it, under which
//! name, and how much it takes from that page.

use photocraft_formats::Availability;

/// The largest file taken from the host page, and the most its files may hold together while they
/// wait to be opened. PhotoCraft has no limit of its own for a file it opens, so this is the other
/// Craft bridges' (WordCraft's). It leaves room in a browser tab, where a file is in memory twice
/// while it opens: once as the page's bytes and once as the copy the app decodes.
pub const MAX_OPEN_BYTES: u64 = 512 << 20;

/// The file name of `path`: the name a saved file is offered to the page under. The web has no
/// folders, so the names PhotoCraft saves under there are bare already; this keeps a folder out
/// of the page's hands should one ever come with a name.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// The extension of the file `name`, as written; none for `name` or `.hidden`.
fn extension(name: &str) -> Option<&str> {
    file_name(name).rsplit_once('.').filter(|(stem, extension)| !stem.is_empty() && !extension.is_empty()).map(|(_, extension)| extension)
}

/// Whether a file PhotoCraft writes as `name` goes to the host page rather than being downloaded:
/// a document or a picture, which is what the page keeps. Those are the files in a format that
/// PhotoCraft's own table (`photocraft_formats`) says this build can write: what Save, Save As,
/// Save a Copy, Export and Save for Web encode. Everything else the app writes stays a download,
/// because a page that keeps documents has no place for it: presets, styles and contours,
/// actions, swatches and colour tables, Camera Raw settings and sidecars, shortcut sets,
/// preferences, the measurement log, and the HTML page Save for Web writes beside its pictures.
pub fn goes_to_page(name: &str) -> bool {
    extension(name).and_then(photocraft_formats::from_extension).is_some_and(|format| format.capabilities().write == Availability::Available)
}

/// Why a file of `size` bytes from the host page is not taken while `waiting` bytes of its files
/// have yet to be opened; `None` when it is taken. The reason reads after "Couldn't open <name>: ".
pub fn open_refusal(size: u64, waiting: u64) -> Option<String> {
    let limit = megabytes(MAX_OPEN_BYTES);
    if size > MAX_OPEN_BYTES {
        return Some(format!("it is {} MB, and a page can send PhotoCraft at most {limit} MB", megabytes(size)));
    }
    if waiting.saturating_add(size) > MAX_OPEN_BYTES {
        return Some(format!("the files sent before it are still waiting to open (PhotoCraft holds at most {limit} MB of them); send it again later"));
    }
    None
}

/// Why a file of `size` bytes is not taken when the browser has no memory left for it. The reason
/// reads after "Couldn't open <name>: ".
pub fn no_memory(size: u64) -> String {
    format!(
        "there is not enough memory to open it (it needs about {} MB). Save your work, close other documents or use Edit › Purge › All, then send it again",
        megabytes(size)
    )
}

/// `bytes` in whole megabytes, rounded up.
fn megabytes(bytes: u64) -> u64 {
    bytes.div_ceil(1 << 20)
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_codecs::{ChannelLayout, EncodeOptions, Format, Image};
    use photocraft_ui_egui::{PhotocraftApp, Services};

    /// The rule that decides where a written file goes, on the names PhotoCraft writes.
    #[test]
    fn documents_and_pictures_go_to_the_page_and_everything_else_is_downloaded() {
        for name in [
            "photo.psd",
            "big.psb",
            "art.pcraft",
            "layers.ora",
            "scan.tif",
            "scan.TIFF",
            "photo.png",
            "photo.jpg",
            "photo.jpeg",
            "photo.webp",
            "anim.gif",
            "icon.bmp",
            "render.exr",
            "sky.hdr",
            "tex.tga",
            "print.pdf",
            "Untitled-1.PSD",
            "images/slice_01.png",
        ] {
            assert!(goes_to_page(name), "{name} is a document or a picture");
        }
        for name in [
            "Presets.pcpresets",
            "Styles.asl",
            "Contours.shc",
            "Default Actions.atn",
            "Swatches.aco",
            "Swatches.ase",
            "Table.act",
            "RAW Develop Settings.xmp",
            "IMG_0001.xmp",
            "Photoshop Defaults.kys",
            "Measurements.csv",
            "page.html",
            "notes.txt",
            "photocraft.json",
            "photo.heic",
            "raw.dng",
            "logo.svg",
        ] {
            assert!(!goes_to_page(name), "{name} stays a download");
        }
        // No extension to tell by, or nothing but one: never taken for a document.
        for name in ["", "document", ".psd", "photo.", "psd", "a.psd/", "dir.psd/readme"] {
            assert!(!goes_to_page(name), "{name:?}");
        }
    }

    #[test]
    fn the_name_offered_to_the_page_is_the_last_part_of_the_path() {
        assert_eq!(file_name("photo.psd"), "photo.psd");
        assert_eq!(file_name("images/slice_01.png"), "slice_01.png");
        assert_eq!(file_name("C:\\work\\photo.psd"), "photo.psd");
        assert_eq!(file_name("folder/"), "");
        assert_eq!(file_name(""), "");
    }

    #[test]
    fn a_file_over_the_limit_or_behind_a_full_queue_is_refused() {
        assert_eq!(open_refusal(0, 0), None);
        assert_eq!(open_refusal(MAX_OPEN_BYTES, 0), None, "the limit itself is taken");
        let too_big = open_refusal(MAX_OPEN_BYTES + 1, 0).expect("one byte over is refused");
        assert!(too_big.contains("513 MB") && too_big.contains("512 MB"), "{too_big}");
        // The same limit holds for everything waiting, so a page can't fill memory between frames.
        assert_eq!(open_refusal(1 << 20, MAX_OPEN_BYTES - (1 << 20)), None);
        assert!(open_refusal(1 << 20, MAX_OPEN_BYTES).is_some_and(|why| why.contains("still waiting to open")));
        assert!(open_refusal(u64::MAX, u64::MAX).is_some(), "sizes never overflow");
        assert!(no_memory(300 << 20).contains("300 MB"));
    }

    /// A 2 x 2 picture encoded as `format`.
    fn encoded(format: Format) -> Vec<u8> {
        let image = Image::from_u8(2, 2, ChannelLayout::Rgba, vec![200; 16]).expect("a 2 x 2 image");
        photocraft_codecs::encode(&image, format, &EncodeOptions::default()).expect("an encoded picture")
    }

    /// `icon` (one picture) as an icon file that lists the picture twice, as two sizes.
    fn with_two_sizes(icon: &[u8]) -> Vec<u8> {
        // The file is a 6-byte header, one 16-byte entry per size, then the pictures.
        let (header, rest) = icon.split_at(6);
        let (entry, pictures) = rest.split_at(16);
        let mut entry = entry.to_vec();
        // The entry ends with the offset of its picture, which the second entry pushes back.
        let offset = u32::from_le_bytes(entry[12..16].try_into().expect("four bytes")) + 16;
        entry[12..16].copy_from_slice(&offset.to_le_bytes());
        let mut two = header.to_vec();
        two[4] = 2;
        two.extend_from_slice(&entry);
        two.extend_from_slice(&entry);
        two.extend_from_slice(pictures);
        two
    }

    /// The file a document has after the page sent it as `name` (`PhotocraftApp::open_from_host`,
    /// with the web shell's importer).
    fn file_of(name: &str, bytes: &[u8]) -> Option<String> {
        let import = |name: &str, bytes: &[u8], options: &photocraft_io::ImportOptions| {
            photocraft_io::import_with_options(name, bytes, options).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string())
        };
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services { import: Some(Box::new(import)), ..Default::default() });
        app.open_from_host(name, bytes).expect("the file opens");
        let document = app.session.active().expect("an open document");
        assert_eq!(document.doc.name, name);
        assert!(!document.is_dirty(), "a file that was only opened has nothing to save");
        document.path.clone()
    }

    /// The name the page sent becomes the document's file only where File › Save can write the
    /// file back whole, so that Save sends the page that same name. Anything else has no file:
    /// with no save dialog on the web, a kept name would be written over without a question.
    #[test]
    fn a_file_from_the_page_keeps_its_name_as_its_file_only_where_save_writes_it_back() {
        let png = encoded(Format::Png);
        assert_eq!(file_of("photo.png", &png).as_deref(), Some("photo.png"));
        assert_eq!(file_of("document", &png), None, "a name that says nothing about the format");
        assert_eq!(file_of("photo.heic", &png), None, "a format PhotoCraft only reads");
        // An icon of two sizes opens as one of them: Save must not write that one over both.
        let icon = encoded(Format::Ico);
        assert_eq!(file_of("icon.ico", &icon).as_deref(), Some("icon.ico"));
        assert_eq!(file_of("icon.ico", &with_two_sizes(&icon)), None);
    }
}
