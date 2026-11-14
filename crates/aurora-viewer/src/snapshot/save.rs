//! Writing a photo to disk on a worker, with its progress
//! (LLViewerWindow::saveImageLocal / LLImageFormatted::save): RGBA from the
//! renderer, RGB in the file (RGBA in the PNG of a cut-out photo,
//! `isolate`), PNG row by row so the window can show how far it is, JPEG
//! at its quality, BMP. The file is written beside its final
//! name and renamed once complete: a photo is never seen half written.
//!
//! A photo may be 16,384 pixels a side, 1 GiB of RGBA: its pixels are shared
//! with the reel's copy (`Arc`), never cloned; the PNG is written from them a
//! band of rows at a time, and only the JPEG and the BMP, whose encoders
//! take the whole image, get one RGB copy, refused with a message when the
//! system has no room for it.

use super::FileFormat;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Side of the thumbnail given back for the gallery.
pub const THUMB_SIDE: u32 = 192;

pub struct Job {
    pub width: u32,
    pub height: u32,
    /// RGBA8, rows top to bottom; shared with the reel's copy.
    pub rgba: Arc<Vec<u8>>,
    pub format: FileFormat,
    pub jpeg_quality: u8,
    pub path: PathBuf,
    /// A cut-out photo: the PNG keeps the alpha, the other formats lay it
    /// on `matte` (sRGB).
    pub alpha: bool,
    pub matte: [u8; 3],
    /// XMP put in a JPEG (a 360° photo's, `pano::xmp`).
    pub xmp: Option<String>,
}

/// What a finished save gives back.
pub struct Saved {
    pub path: PathBuf,
    pub bytes: u64,
    /// RGBA8 thumbnail (width, height, pixels) for the gallery.
    pub thumb: (u32, u32, Vec<u8>),
    pub millis: u128,
}

/// A save in progress.
pub struct Saving {
    progress: Arc<AtomicU32>,
    rx: crossbeam_channel::Receiver<Result<Saved, String>>,
}

impl Saving {
    pub fn start(job: Job) -> Saving {
        Saving::start_with(move |p| save(job, p))
    }

    /// A save whose picture is made on the worker first (a 360° photo
    /// joined from its views): `work` reports its progress and saves.
    pub fn start_with(work: impl FnOnce(&AtomicU32) -> Result<Saved, String> + Send + 'static) -> Saving {
        let progress = Arc::new(AtomicU32::new(0));
        let (tx, rx) = crossbeam_channel::bounded(1);
        let p = progress.clone();
        let spawned = std::thread::Builder::new().name("aurora-photo-save".into()).spawn(move || {
            let _ = tx.send(work(&p));
        });
        if let Err(e) = spawned {
            log::warn!("photo: cannot start the save thread: {e}");
        }
        Saving { progress, rx }
    }

    /// 0..=1.
    pub fn fraction(&self) -> f32 {
        match self.progress.load(Ordering::Relaxed) {
            CANCELLED => 0.0,
            p => p as f32 / 10_000.0,
        }
    }

    /// Gives the save up (the session ends, FireDragon's `fdCancelSave`):
    /// the worker stops at its next band of rows, removes its `.part` file
    /// and ends with an error nobody reads. A photo already renamed to its
    /// final name stays.
    pub fn cancel(&self) {
        self.progress.store(CANCELLED, Ordering::Relaxed);
    }

    pub fn poll(&self) -> Option<Result<Saved, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(crossbeam_channel::TryRecvError::Empty) => None,
            Err(crossbeam_channel::TryRecvError::Disconnected) => Some(Err("save thread ended".into())),
        }
    }
}

/// The progress value of a save given up (`Saving::cancel`): the worker's
/// own updates no longer replace it.
pub const CANCELLED: u32 = u32::MAX;

fn set(progress: &AtomicU32, f: f32) {
    let v = (f.clamp(0.0, 1.0) * 10_000.0) as u32;
    let _ = progress.try_update(Ordering::Relaxed, Ordering::Relaxed, |c| (c != CANCELLED).then_some(v));
}

fn cancelled(progress: &AtomicU32) -> Result<(), String> {
    if progress.load(Ordering::Relaxed) == CANCELLED {
        Err("photo save cancelled".into())
    } else {
        Ok(())
    }
}

pub fn save(job: Job, progress: &AtomicU32) -> Result<Saved, String> {
    let t0 = std::time::Instant::now();
    let Job {
        width,
        height,
        rgba,
        format,
        jpeg_quality,
        path,
        alpha,
        matte,
        xmp,
    } = job;
    if rgba.len() as u64 != super::photo_bytes((width, height)) {
        return Err(format!("photo: {} bytes for {width} x {height}", rgba.len()));
    }
    let thumb = thumbnail(width, height, &rgba);
    set(progress, 0.05);
    let keep_alpha = alpha && format == FileFormat::Png;
    // the PNG is written from the RGBA itself; the other encoders take the
    // whole image as RGB
    let rgb: Option<Vec<u8>> = if format == FileFormat::Png {
        None
    } else {
        Some(rgb_of(&rgba, alpha.then_some(matte))?)
    };
    set(progress, 0.1);
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    }
    let part = part_path(&path);
    let written = (|| -> Result<(), String> {
        let file = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
        let mut out = BufWriter::with_capacity(1 << 20, file);
        match &rgb {
            Some(rgb) if format == FileFormat::Jpeg && xmp.is_some() => {
                let mut jpeg = Vec::new();
                encode(&mut jpeg, width, height, rgb, format, jpeg_quality, progress)?;
                let jpeg = xmp.as_deref().and_then(|x| super::pano::with_xmp(&jpeg, x)).unwrap_or(jpeg);
                out.write_all(&jpeg).map_err(|e| e.to_string())?;
            }
            Some(rgb) => encode(&mut out, width, height, rgb, format, jpeg_quality, progress)?,
            None if keep_alpha => encode(&mut out, width, height, &rgba, format, jpeg_quality, progress)?,
            None => encode_png_opaque(&mut out, width, height, &rgba, progress)?,
        }
        out.flush().map_err(|e| e.to_string())?;
        out.into_inner().map_err(|e| e.to_string())?.sync_all().map_err(|e| e.to_string())
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    if let Err(e) = cancelled(progress) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, &path).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("{}: {e}", path.display())
    })?;
    set(progress, 1.0);
    let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
    Ok(Saved {
        path,
        bytes,
        thumb,
        millis: t0.elapsed().as_millis(),
    })
}

fn part_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".part");
    path.with_file_name(name)
}

/// The RGB of RGBA pixels in a buffer of its own, laid on `matte` (a
/// cut-out photo, `extras::lay_on_matte`), or the message shown when the
/// system has no memory for it (three quarters of the photo's size).
fn rgb_of(rgba: &[u8], matte: Option<[u8; 3]>) -> Result<Vec<u8>, String> {
    let mut rgb = Vec::new();
    rgb.try_reserve_exact(rgba.len() / 4 * 3)
        .map_err(|_| aurora_i18n::tr!("p16-memory").to_owned())?;
    match matte {
        Some(m) => rgb.extend(rgba.as_chunks::<4>().0.iter().flat_map(|p| {
            let mut px = *p;
            super::extras::lay_on_matte(&mut px, m);
            [px[0], px[1], px[2]]
        })),
        None => rgb.extend(rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]])),
    }
    Ok(rgb)
}

/// Rows of a band the PNG is written by.
const PNG_BAND: usize = 64;

/// A PNG without alpha written from RGBA8 rows, a band at a time through a
/// small RGB buffer: no second copy of a large photo. Progress as
/// [`encode`].
pub fn encode_png_opaque(out: &mut impl Write, width: u32, height: u32, rgba: &[u8], progress: &AtomicU32) -> Result<(), String> {
    let mut enc = png::Encoder::new(&mut *out, width, height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    let mut stream = writer.stream_writer().map_err(|e| e.to_string())?;
    let row = width as usize * 4;
    let mut rgb = Vec::with_capacity(width as usize * 3 * PNG_BAND);
    for (i, band) in rgba.chunks(row.max(4) * PNG_BAND).enumerate() {
        rgb.clear();
        rgb.extend(band.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]));
        cancelled(progress)?;
        stream.write_all(&rgb).map_err(|e| e.to_string())?;
        let rows = ((i + 1) * PNG_BAND).min(height as usize);
        set(progress, 0.1 + 0.88 * rows as f32 / height.max(1) as f32);
    }
    stream.finish().map_err(|e| e.to_string())
}

/// Encode RGB8 rows (or RGBA8 ones for a PNG, by their length); progress
/// from 0.1 to 0.98.
pub fn encode(
    out: &mut impl Write,
    width: u32,
    height: u32,
    rgb: &[u8],
    format: FileFormat,
    jpeg_quality: u8,
    progress: &AtomicU32,
) -> Result<(), String> {
    match format {
        FileFormat::Png => {
            let channels = if rgb.len() == width as usize * height as usize * 4 { 4 } else { 3 };
            let mut enc = png::Encoder::new(&mut *out, width, height);
            enc.set_color(if channels == 4 { png::ColorType::Rgba } else { png::ColorType::Rgb });
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::Fast);
            let mut writer = enc.write_header().map_err(|e| e.to_string())?;
            let mut stream = writer.stream_writer().map_err(|e| e.to_string())?;
            let row = width as usize * channels;
            // a band of rows at a time, the progress after each
            for (i, band) in rgb.chunks(row * 64).enumerate() {
                cancelled(progress)?;
                stream.write_all(band).map_err(|e| e.to_string())?;
                let rows = ((i + 1) * 64).min(height as usize);
                set(progress, 0.1 + 0.88 * rows as f32 / height.max(1) as f32);
            }
            stream.finish().map_err(|e| e.to_string())
        }
        FileFormat::Jpeg => {
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut *out, jpeg_quality.clamp(1, 100));
            let r = image::ImageEncoder::write_image(enc, rgb, width, height, image::ExtendedColorType::Rgb8);
            set(progress, 0.98);
            r.map_err(|e| e.to_string())
        }
        FileFormat::Bmp => {
            let enc = image::codecs::bmp::BmpEncoder::new(&mut *out);
            let r = image::ImageEncoder::write_image(enc, rgb, width, height, image::ExtendedColorType::Rgb8);
            set(progress, 0.98);
            r.map_err(|e| e.to_string())
        }
    }
}

/// A JPEG in memory: a postcard (LLPanelSnapshotPostcard's
/// SNAPSHOT_FORMAT_JPEG).
pub fn encode_jpeg(width: u32, height: u32, rgba: &[u8], quality: u8) -> Result<Vec<u8>, String> {
    if rgba.len() != width as usize * height as usize * 4 {
        return Err(format!("photo: {} bytes for {width} x {height}", rgba.len()));
    }
    let rgb: Vec<u8> = rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    encode(&mut out, width, height, &rgb, FileFormat::Jpeg, quality, &AtomicU32::new(0))?;
    Ok(out)
}

/// A small copy for the gallery, its long side THUMB_SIDE.
pub fn thumbnail(width: u32, height: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
    downscale(width, height, rgba, THUMB_SIDE)
}

/// A copy whose long side is at most `max_side` (box filter over whole
/// pixels: fast, and a photo reduced several times needs no better). The
/// colour is weighted by the alpha: the empty pixels of a cut-out photo
/// (black, transparent) darken none of its edges.
pub fn downscale(width: u32, height: u32, rgba: &[u8], max_side: u32) -> (u32, u32, Vec<u8>) {
    let scale = max_side as f32 / width.max(height).max(1) as f32;
    if scale >= 1.0 {
        return (width, height, rgba.to_vec());
    }
    let tw = ((width as f32 * scale).round() as u32).max(1);
    let th = ((height as f32 * scale).round() as u32).max(1);
    let mut out = vec![0u8; tw as usize * th as usize * 4];
    for ty in 0..th {
        // (64 bits: a side of 16,384 times another overflows 32)
        let at = |t: u32, side: u32, of: u32| (u64::from(t) * u64::from(side) / u64::from(of)) as u32;
        let y0 = at(ty, height, th);
        let y1 = at(ty + 1, height, th).max(y0 + 1).min(height);
        for tx in 0..tw {
            let x0 = at(tx, width, tw);
            let x1 = at(tx + 1, width, tw).max(x0 + 1).min(width);
            let mut sum = [0u64; 4];
            for y in y0..y1 {
                let start = (y as usize * width as usize + x0 as usize) * 4;
                for px in rgba[start..start + (x1 - x0) as usize * 4].as_chunks::<4>().0 {
                    let a = u64::from(px[3]);
                    for c in 0..3 {
                        sum[c] += u64::from(px[c]) * a;
                    }
                    sum[3] += a;
                }
            }
            let n = u64::from((y1 - y0) * (x1 - x0)).max(1);
            let o = (ty as usize * tw as usize + tx as usize) * 4;
            for c in 0..3 {
                out[o + c] = sum[c].checked_div(sum[3]).unwrap_or(0) as u8;
            }
            out[o + 3] = (sum[3] / n) as u8;
        }
    }
    (tw, th, out)
}
