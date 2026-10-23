//! Writes `vitals_core::Image` (BGRA, straight alpha) as an RGBA PNG.

use std::{
    fs::File,
    io::BufWriter,
    path::Path,
};

use anyhow::{Context, Result};
use vitals_core::Image;

pub fn write_png(path: &Path, image: &Image) -> Result<()> {
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().context("writing PNG header")?;
    let mut rgba = Vec::with_capacity(image.data.len());
    for pixel in image.data.chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    writer.write_image_data(&rgba).context("writing PNG pixels")?;
    Ok(())
}
