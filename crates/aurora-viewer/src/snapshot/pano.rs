//! 360° photos (the Legacy Viewer's 360 Snapshot: LLFloater360Capture,
//! indra/newview/llfloater360capture.cpp, originally LGPL 2.1): six square
//! views of 90° around the camera, a cube, joined into one equirectangular
//! picture twice as wide as high, saved as a JPEG whose XMP tells the
//! photo sites and viewers it is a panorama (GPano).
//!
//! As there: the quality is the side of a view (128 to 2048, Preview by
//! default), the avatars can be left out (360CaptureHideAvatars, here with
//! what they wear, their particles and their lights: the isolation's
//! background alone), the JPEG's quality is 95 (360CaptureJPEGEncode-
//! Quality) and the world holds still while the six are drawn
//! (freezeWorld).
//!
//! Differences: the views are drawn as any photo is (`app::snapshot`), in
//! tiles within the video memory and the GPU's time (`GpuCost`) and at the
//! supersampling chosen, each wider than 90° by a guard band
//! (`TILE_GUARD`) so that the screen-space effects (ambient occlusion,
//! bloom, contact shadows) see past its edges and meet the next view's
//! without a seam; the six are joined on a worker, not in a web page. The
//! picture is four views wide (no enlargement of the views: a sharper
//! Preview, a smaller file) up to 16,384 pixels, with a fifth quality of
//! 4,096 a view. The views turn with the camera, so the picture's middle is
//! where it looked; its compass heading goes in the XMP.

use super::{MAX_SIDE, TILE_GUARD};
use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

/// JPEG quality of the picture (360CaptureJPEGEncodeQuality).
pub const JPEG_QUALITY: u8 = 95;

/// The side of the views (360QualitySelection).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    #[default]
    Preview,
    Medium,
    High,
    Maximum,
    /// Aurora's: a picture of 16,384 x 8,192.
    Ultra,
}

impl Quality {
    pub const ALL: [Quality; 5] = [Quality::Preview, Quality::Medium, Quality::High, Quality::Maximum, Quality::Ultra];

    /// Pixels on a side of each view.
    pub fn face(self) -> u32 {
        match self {
            Quality::Preview => 128,
            Quality::Medium => 512,
            Quality::High => 1024,
            Quality::Maximum => 2048,
            Quality::Ultra => 4096,
        }
    }
}

/// The equirectangular picture of views of `face` pixels: four views
/// around, half as high.
pub fn output_size(face: u32) -> (u32, u32) {
    let width = (face * 4).min(MAX_SIDE);
    (width, width / 2)
}

/// The side a view is drawn at: its own and the guard band on each side.
pub fn drawn_side(face: u32) -> u32 {
    face + 2 * TILE_GUARD
}

/// The vertical field of view a view is drawn with: 90° over its own
/// pixels, wider by the band.
pub fn drawn_fov(face: u32) -> f32 {
    2.0 * (drawn_side(face) as f32 / face.max(1) as f32).atan()
}

/// The six views, in the order they are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Front,
    Right,
    Back,
    Left,
    Up,
    Down,
}

impl Face {
    pub const ALL: [Face; 6] = [Face::Front, Face::Right, Face::Back, Face::Left, Face::Up, Face::Down];

    /// Where the view looks and its up, in the camera's level frame (x to
    /// its right, y ahead, z up).
    fn local(self) -> (Vec3, Vec3) {
        match self {
            Face::Front => (Vec3::Y, Vec3::Z),
            Face::Right => (Vec3::X, Vec3::Z),
            Face::Back => (Vec3::NEG_Y, Vec3::Z),
            Face::Left => (Vec3::NEG_X, Vec3::Z),
            Face::Up => (Vec3::Z, Vec3::NEG_Y),
            Face::Down => (Vec3::NEG_Z, Vec3::Y),
        }
    }
}

/// The camera's level frame: ahead (its direction made horizontal), its
/// right, and up (the region's +Z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Basis {
    pub ahead: Vec3,
    pub right: Vec3,
}

impl Basis {
    /// From the camera's direction (straight up or down: north).
    pub fn new(forward: Vec3) -> Self {
        let ahead = Vec3::new(forward.x, forward.y, 0.0).try_normalize().unwrap_or(Vec3::Y);
        Basis {
            ahead,
            right: ahead.cross(Vec3::Z),
        }
    }

    fn world(&self, local: Vec3) -> Vec3 {
        self.right * local.x + self.ahead * local.y + Vec3::Z * local.z
    }

    /// The view of `face` from `eye`, and its projection for views of
    /// `face_side` pixels (the camera's, reversed and infinite).
    pub fn matrices(&self, face: Face, eye: Vec3, face_side: u32, near: f32) -> (Mat4, Mat4) {
        let (dir, up) = face.local();
        let view = glam::camera::rh::view::look_to_mat4(eye, self.world(dir), self.world(up));
        let proj = glam::camera::rh::proj::directx::perspective_infinite_reverse(drawn_fov(face_side), 1.0, near);
        (view, proj)
    }

    /// Compass heading of the picture's middle, degrees from north (+Y)
    /// towards east (+X).
    pub fn heading(&self) -> f32 {
        self.ahead.x.atan2(self.ahead.y).to_degrees().rem_euclid(360.0)
    }
}

/// Where a direction of the camera's level frame falls: the view and the
/// pixel of it (continuous, from the top left of the view as drawn, of
/// `drawn` pixels a side for `face` of its own).
fn locate(d: Vec3, face: u32, drawn: u32) -> (usize, f32, f32) {
    let a = d.abs();
    let index = if a.x >= a.y && a.x >= a.z {
        if d.x > 0.0 { 1 } else { 3 }
    } else if a.y >= a.z {
        if d.y > 0.0 { 0 } else { 2 }
    } else if d.z > 0.0 {
        4
    } else {
        5
    };
    let (dir, up) = Face::ALL[index].local();
    let right = dir.cross(up);
    let along = d.dot(dir).max(1e-6);
    // the tangents over the view's own half side, drawn `drawn / face` wider
    let shrink = face as f32 / drawn as f32;
    let (u, v) = (d.dot(right) / along * shrink, d.dot(up) / along * shrink);
    let n = drawn as f32;
    (index, (u + 1.0) * 0.5 * n - 0.5, (1.0 - v) * 0.5 * n - 0.5)
}

/// The RGB of a view at a continuous pixel, between its four nearest.
fn bilinear(rgba: &[u8], side: u32, x: f32, y: f32) -> [f32; 3] {
    let max = side as f32 - 1.0;
    let (x, y) = (x.clamp(0.0, max), y.clamp(0.0, max));
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(side as usize - 1), (y0 + 1).min(side as usize - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |x: usize, y: usize, c: usize| f32::from(rgba[(y * side as usize + x) * 4 + c]);
    let mut out = [0.0; 3];
    for (c, o) in out.iter_mut().enumerate() {
        let top = at(x0, y0, c) * (1.0 - fx) + at(x1, y0, c) * fx;
        let bottom = at(x0, y1, c) * (1.0 - fx) + at(x1, y1, c) * fx;
        *o = top * (1.0 - fy) + bottom * fy;
    }
    out
}

/// The six views (`Face::ALL`'s order, RGBA of `drawn_side(face)` pixels a
/// side) joined into the equirectangular picture of `out` (RGBA, opaque):
/// its middle ahead, its top straight up. None when the views are not that
/// size or the system has no memory for the picture. Its rows are joined
/// in parallel.
pub fn stitch(faces: &[Vec<u8>], face: u32, out: (u32, u32)) -> Option<Vec<u8>> {
    use rayon::prelude::*;
    let drawn = drawn_side(face);
    let bytes = super::photo_bytes((drawn, drawn));
    if faces.len() != 6 || faces.iter().any(|f| f.len() as u64 != bytes) || out.0 == 0 || out.1 == 0 {
        return None;
    }
    let mut picture = super::photo_buffer(super::photo_bytes(out))?;
    let (w, h) = (out.0 as usize, out.1 as usize);
    picture.par_chunks_mut(w * 4).enumerate().for_each(|(row, line)| {
        let lat = std::f32::consts::FRAC_PI_2 - (row as f32 + 0.5) / h as f32 * std::f32::consts::PI;
        let (sin_lat, cos_lat) = lat.sin_cos();
        for (col, px) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let lon = (col as f32 + 0.5) / w as f32 * std::f32::consts::TAU - std::f32::consts::PI;
            let (sin_lon, cos_lon) = lon.sin_cos();
            let d = Vec3::new(cos_lat * sin_lon, cos_lat * cos_lon, sin_lat);
            let (index, x, y) = locate(d, face, drawn);
            let rgb = bilinear(&faces[index], drawn, x, y);
            for c in 0..3 {
                px[c] = rgb[c].round().clamp(0.0, 255.0) as u8;
            }
            px[3] = 255;
        }
    });
    Some(picture)
}

/// The XMP that makes the picture a panorama for the photo sites and the
/// viewers (Google's Photo Sphere XMP: GPano), the middle at `heading`.
pub fn xmp(size: (u32, u32), heading: f32) -> String {
    let heading = heading.rem_euclid(360.0);
    format!(
        concat!(
            "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>",
            "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">",
            "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">",
            "<rdf:Description rdf:about=\"\" xmlns:GPano=\"http://ns.google.com/photos/1.0/panorama/\">",
            "<GPano:ProjectionType>equirectangular</GPano:ProjectionType>",
            "<GPano:UsePanoramaViewer>True</GPano:UsePanoramaViewer>",
            "<GPano:FullPanoWidthPixels>{w}</GPano:FullPanoWidthPixels>",
            "<GPano:FullPanoHeightPixels>{h}</GPano:FullPanoHeightPixels>",
            "<GPano:CroppedAreaImageWidthPixels>{w}</GPano:CroppedAreaImageWidthPixels>",
            "<GPano:CroppedAreaImageHeightPixels>{h}</GPano:CroppedAreaImageHeightPixels>",
            "<GPano:CroppedAreaLeftPixels>0</GPano:CroppedAreaLeftPixels>",
            "<GPano:CroppedAreaTopPixels>0</GPano:CroppedAreaTopPixels>",
            "<GPano:PoseHeadingDegrees>{heading:.1}</GPano:PoseHeadingDegrees>",
            "<GPano:InitialViewHeadingDegrees>{heading:.0}</GPano:InitialViewHeadingDegrees>",
            "</rdf:Description></rdf:RDF></x:xmpmeta>",
            "<?xpacket end=\"w\"?>"
        ),
        w = size.0,
        h = size.1,
        heading = heading,
    )
}

/// The XMP namespace that opens its APP1 segment.
const XMP_NS: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

/// A JPEG with `xmp` in an APP1 segment right after its start; None for
/// what is not a JPEG or an XMP too long for one segment.
pub fn with_xmp(jpeg: &[u8], xmp: &str) -> Option<Vec<u8>> {
    if jpeg.len() < 2 || jpeg[..2] != [0xFF, 0xD8] {
        return None;
    }
    let length = u16::try_from(2 + XMP_NS.len() + xmp.len()).ok()?;
    let mut out = Vec::with_capacity(jpeg.len() + usize::from(length) + 2);
    out.extend_from_slice(&jpeg[..2]);
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(XMP_NS);
    out.extend_from_slice(xmp.as_bytes());
    out.extend_from_slice(&jpeg[2..]);
    Some(out)
}

/// RLVa camera restrictions a 360° photo would see past: its views look
/// every way with a field wider than any allowed, from where the camera
/// is. While one is active it is not taken.
pub fn rlva_blocks(has: impl Fn(&str) -> bool) -> bool {
    [
        "setcam",
        "setcam_fovmin",
        "setcam_fovmax",
        "setcam_textures",
        "setcam_avdistmax",
        "setcam_origindistmax",
        "camdrawmin",
        "camdrawmax",
    ]
    .into_iter()
    .any(has)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_is_four_views_wide_and_half_as_high() {
        assert_eq!(output_size(Quality::Preview.face()), (512, 256));
        assert_eq!(output_size(Quality::Maximum.face()), (8192, 4096));
        assert_eq!(output_size(Quality::Ultra.face()), (MAX_SIDE, MAX_SIDE / 2));
        assert_eq!(Quality::default(), Quality::Preview, "the Legacy Viewer's default");
        // 90° over the view's own pixels, the band beyond
        let face = 1024;
        let half = (drawn_fov(face) * 0.5).tan();
        assert!((half * face as f32 - drawn_side(face) as f32).abs() < 0.01);
    }

    #[test]
    fn the_views_turn_with_the_camera_and_name_its_heading() {
        let east = Basis::new(Vec3::new(1.0, 0.0, -0.5));
        assert!((east.heading() - 90.0).abs() < 1e-3);
        assert!(east.right.abs_diff_eq(Vec3::NEG_Y, 1e-6), "east, the right is south");
        assert!((Basis::new(Vec3::NEG_Y).heading() - 180.0).abs() < 1e-3);
        assert_eq!(Basis::new(Vec3::Z), Basis::new(Vec3::Y), "straight up: north");
    }

    /// The pixel a world direction is drawn at in a view, by the matrices
    /// the frame is drawn with.
    fn drawn_at(basis: &Basis, face: Face, side: u32, d: Vec3) -> Option<(f32, f32)> {
        let (view, proj) = basis.matrices(face, Vec3::new(10.0, 20.0, 30.0), side, 0.1);
        let clip = proj * view * (Vec3::new(10.0, 20.0, 30.0) + d * 50.0).extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let (x, y) = (clip.x / clip.w, clip.y / clip.w);
        let n = drawn_side(side) as f32;
        ((-1.0..=1.0).contains(&x) && (-1.0..=1.0).contains(&y)).then_some(((x + 1.0) * 0.5 * n - 0.5, (1.0 - y) * 0.5 * n - 0.5))
    }

    #[test]
    fn the_joining_finds_each_direction_where_its_view_drew_it() {
        let basis = Basis::new(Vec3::new(0.6, 0.8, 0.2));
        let side = 256;
        let mut checked = 0;
        for i in 0..400 {
            // directions all around, the cube's edges and corners among them
            let t = i as f32 * 0.37;
            let local = Vec3::new(t.sin(), (t * 1.7).cos(), (t * 0.61).sin() * 1.3).normalize();
            let (index, x, y) = locate(local, side, drawn_side(side));
            let world = basis.right * local.x + basis.ahead * local.y + Vec3::Z * local.z;
            let (dx, dy) = drawn_at(&basis, Face::ALL[index], side, world).expect("in its view");
            assert!(
                (dx - x).abs() < 0.05 && (dy - y).abs() < 0.05,
                "{i}: {local} ({x}, {y}) drawn at ({dx}, {dy})"
            );
            // within the view's own pixels: the band is only read across
            let lo = TILE_GUARD as f32 - 0.6;
            let hi = (TILE_GUARD + side) as f32 - 0.4;
            assert!((lo..=hi).contains(&x) && (lo..=hi).contains(&y), "{i}: ({x}, {y})");
            checked += 1;
        }
        assert_eq!(checked, 400);
    }

    #[test]
    fn each_view_lands_in_its_place_of_the_picture() {
        let face = 16;
        let side = drawn_side(face);
        let colors = [[200u8, 0, 0], [0, 200, 0], [0, 0, 200], [200, 200, 0], [0, 200, 200], [200, 0, 200]];
        let faces: Vec<Vec<u8>> = colors
            .iter()
            .map(|c| (0..side * side).flat_map(|_| [c[0], c[1], c[2], 255]).collect())
            .collect();
        let out = output_size(face);
        let pic = stitch(&faces, face, out).expect("joined");
        let at = |x: u32, y: u32| {
            let i = ((y * out.0 + x) * 4) as usize;
            [pic[i], pic[i + 1], pic[i + 2]]
        };
        let (w, h) = out;
        assert_eq!(at(w / 2, h / 2), colors[0], "the middle: ahead");
        assert_eq!(at(w * 3 / 4, h / 2), colors[1], "a quarter turn right");
        assert_eq!(at(0, h / 2), colors[2], "the edges: behind");
        assert_eq!(at(w / 4, h / 2), colors[3], "a quarter turn left");
        assert_eq!(at(w / 2, 0), colors[4], "the top row: up");
        assert_eq!(at(w / 2, h - 1), colors[5], "the bottom row: down");
        assert!(pic.chunks(4).all(|p| p[3] == 255));
        assert!(stitch(&faces[..5], face, out).is_none(), "five views");
    }

    #[test]
    fn the_xmp_goes_in_right_after_the_start_of_the_jpeg() {
        let x = xmp((8192, 4096), 450.0);
        assert!(x.contains("<GPano:ProjectionType>equirectangular</GPano:ProjectionType>"));
        assert!(x.contains("<GPano:FullPanoWidthPixels>8192<") && x.contains("<GPano:CroppedAreaImageHeightPixels>4096<"));
        assert!(x.contains("<GPano:PoseHeadingDegrees>90.0<"));
        let jpeg = [0xFF, 0xD8, 0xFF, 0xDB, 1, 2, 3, 0xFF, 0xD9];
        let out = with_xmp(&jpeg, &x).expect("a JPEG");
        assert_eq!(&out[..4], &[0xFF, 0xD8, 0xFF, 0xE1]);
        let len = usize::from(u16::from_be_bytes([out[4], out[5]]));
        assert_eq!(len, 2 + XMP_NS.len() + x.len());
        assert_eq!(&out[6..6 + XMP_NS.len()], XMP_NS);
        assert_eq!(&out[4 + len..], &jpeg[2..], "the rest as it was");
        assert!(with_xmp(b"\x89PNG", &x).is_none());
    }

    #[test]
    fn rlva_camera_limits_stop_the_360_photo() {
        assert!(!rlva_blocks(|_| false));
        assert!(rlva_blocks(|b| b == "setcam_fovmax"));
        assert!(rlva_blocks(|b| b == "setcam"));
        assert!(!rlva_blocks(|b| b == "shownames"));
    }
}
