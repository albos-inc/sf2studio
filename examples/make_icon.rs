//! Renders `assets/icon.svg` into the app icons:
//!
//! - `assets/icon-<size>.png` for 16…1024 px
//! - `assets/icon-256.rgba`, the window icon embedded in the app
//! - `assets/sf2studio.ico` (Windows) and, on macOS, `assets/sf2studio.icns`
//!
//! ```text
//! cargo run --example make_icon
//! ```
//!
//! To use other artwork, replace `assets/icon.svg` (1024 × 1024) and run it
//! again.

use std::path::Path;

use resvg::{tiny_skia, usvg};

const SIZES: [u32; 9] = [16, 32, 48, 64, 128, 256, 512, 1024, 24];

fn render(tree: &usvg::Tree, size: u32) -> tiny_skia::Pixmap {
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("icon size");
    let scale = size as f32 / tree.size().width();
    resvg::render(tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap
}

/// Straight (not premultiplied) RGBA.
fn unpremultiplied(pixmap: &tiny_skia::Pixmap) -> Vec<u8> {
    let mut rgba = pixmap.data().to_vec();
    for pixel in rgba.chunks_mut(4) {
        let alpha = pixel[3] as u32;
        if alpha > 0 && alpha < 255 {
            for c in &mut pixel[..3] {
                *c = ((*c as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    rgba
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let svg = std::fs::read(assets.join("icon.svg"))?;
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default())?;

    let mut pngs = Vec::new();
    for size in SIZES {
        let pixmap = render(&tree, size);
        let png = pixmap.encode_png()?;
        if size != 24 {
            std::fs::write(assets.join(format!("icon-{size}.png")), &png)?;
        }
        if size == 256 {
            std::fs::write(assets.join("icon-256.rgba"), unpremultiplied(&pixmap))?;
        }
        pngs.push((size, png));
    }

    // Windows: an ICO holding PNG images.
    let ico_sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let images: Vec<&(u32, Vec<u8>)> =
        ico_sizes.iter().filter_map(|s| pngs.iter().find(|(size, _)| size == s)).collect();
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, png) in &images {
        let side = if *size >= 256 { 0 } else { *size as u8 };
        ico.extend_from_slice(&[side, side, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in &images {
        ico.extend_from_slice(png);
    }
    std::fs::write(assets.join("sf2studio.ico"), ico)?;

    // macOS: an ICNS made by iconutil from an iconset.
    if cfg!(target_os = "macos") {
        let iconset = std::env::temp_dir().join("sf2studio.iconset");
        let _ = std::fs::remove_dir_all(&iconset);
        std::fs::create_dir_all(&iconset)?;
        for (name, size) in [
            ("icon_16x16", 16),
            ("icon_16x16@2x", 32),
            ("icon_32x32", 32),
            ("icon_32x32@2x", 64),
            ("icon_128x128", 128),
            ("icon_128x128@2x", 256),
            ("icon_256x256", 256),
            ("icon_256x256@2x", 512),
            ("icon_512x512", 512),
            ("icon_512x512@2x", 1024),
        ] {
            let png = &pngs.iter().find(|(s, _)| *s == size).expect("rendered size").1;
            std::fs::write(iconset.join(format!("{name}.png")), png)?;
        }
        let status = std::process::Command::new("iconutil")
            .args(["-c", "icns"])
            .arg(&iconset)
            .arg("-o")
            .arg(assets.join("sf2studio.icns"))
            .status()?;
        if !status.success() {
            return Err("iconutil failed".into());
        }
    }
    println!("wrote the icons to {}", assets.display());
    Ok(())
}
