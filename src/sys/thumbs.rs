//! Small previews of images (wallpapers), decoded on background threads.

use std::path::{Path, PathBuf};

/// Decode `path` and scale it to fit `max_w`×`max_h`.
pub fn load(path: &Path, max_w: u32, max_h: u32) -> Option<egui::ColorImage> {
    let img = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let img = img.thumbnail(max_w, max_h).to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

/// Decode several images one after another, sending each as it's ready.
pub fn load_all(paths: Vec<PathBuf>, max_w: u32, max_h: u32, ctx: egui::Context) -> flume::Receiver<(PathBuf, Option<egui::ColorImage>)> {
    let (tx, rx) = flume::unbounded();
    std::thread::spawn(move || {
        for p in paths {
            let img = load(&p, max_w, max_h);
            if tx.send((p, img)).is_err() {
                break;
            }
            ctx.request_repaint();
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_to_fit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wide.png");
        image::RgbaImage::from_pixel(400, 100, image::Rgba([10, 20, 30, 255])).save(&path).unwrap();
        let t = load(&path, 160, 90).unwrap();
        assert_eq!(t.size, [160, 40]);
        assert!(load(&dir.path().join("missing.png"), 160, 90).is_none());
        std::fs::write(dir.path().join("bad.png"), "not an image").unwrap();
        assert!(load(&dir.path().join("bad.png"), 160, 90).is_none());
    }
}
