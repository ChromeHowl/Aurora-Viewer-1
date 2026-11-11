//! Each avatar's render time, for the auto-tune (settings/render_photo.rs).
//!
//! The Legacy Viewer times each avatar's draws on the GPU
//! (LLVOAvatar::readProfileQuery, mGPURenderTime). Aurora draws the scene
//! from GPU-built lists, with no per-avatar timer: an avatar's share of the
//! last frame's GPU time is estimated by its share of the triangles drawn,
//! its body and what it wears (HUDs left out). Cheap: one walk over the
//! scene's objects, once a second.

use super::Scene;
use std::collections::HashMap;

/// An avatar's estimated render time (µs): `gpu_ms` of a frame that drew
/// `frame_triangles`, shared by the avatar's `triangles`.
pub fn estimate_us(gpu_ms: f32, frame_triangles: u64, triangles: u64) -> f32 {
    if frame_triangles == 0 || !gpu_ms.is_finite() || gpu_ms <= 0.0 {
        return 0.0;
    }
    gpu_ms * 1000.0 * (triangles.min(frame_triangles) as f32 / frame_triangles as f32)
}

impl Scene {
    /// Triangles of each avatar (by object index): its body's faces and
    /// those of every object it wears, HUDs left out.
    pub fn avatar_triangles(&self) -> HashMap<usize, u64> {
        let mut out: HashMap<usize, u64> = HashMap::new();
        for (idx, g) in self.gpu.iter().enumerate() {
            let owner = if g.is_avatar {
                Some(idx)
            } else {
                g.owner_avatar.filter(|_| !g.hud)
            };
            let Some(owner) = owner else {
                continue;
            };
            let triangles: u64 = g.faces.iter().map(|f| u64::from(f.cmd.index_count / 3)).sum();
            *out.entry(owner).or_default() += triangles;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_time_is_shared_by_triangles() {
        assert_eq!(estimate_us(10.0, 1_000_000, 250_000), 2_500.0);
        assert_eq!(estimate_us(10.0, 0, 250_000), 0.0, "nothing drawn");
        assert_eq!(estimate_us(f32::NAN, 1_000, 10), 0.0);
        assert_eq!(estimate_us(10.0, 1_000, 5_000), 10_000.0, "never more than the frame");
    }
}
