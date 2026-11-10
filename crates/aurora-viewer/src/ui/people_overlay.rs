//! What People draws over the world around us: the chat range spheres
//! (FSShowChatRangeSpheres) and, in mouselook, the markers of the residents
//! in range that are out of view (FSMouselookCombatFeatures with
//! ExodusMouselookIFF and ExodusMouselookIFFRange).
//!
//! Port of drawChatRangeSpheres / renderChatRangeSphere
//! (indra/newview/llviewerdisplay.cpp) and of the IFF part of
//! LLViewerWindow::draw with LLTracker::drawMarker (llviewerwindow.cpp,
//! lltracker.cpp), originally LGPL 2.1.
//!
//! Differences: the spheres are drawn as a light wire frame (rings of
//! latitude and meridians) in the mini-map's ring colours
//! (MapWhisperRingColor, MapChatRingColor, MapShoutRingColor) over the
//! world, where the Legacy Viewer fills translucent spheres tested against
//! the depth: a label layer has no depth, and a wire hides less of a scene.
//! The crosshair's name and distance of the combat features are not drawn.

use super::hud::Projector;
use crate::theme::Palette;
use crate::world::World;
use egui::{Color32, Pos2, Rect, Stroke, vec2};
use glam::Vec3;

/// Segments of a ring of a sphere.
const SEGMENTS: usize = 48;
/// Rings of latitude (the equator in the middle) and meridians of a sphere.
const LATITUDES: usize = 5;
const MERIDIANS: usize = 8;

/// The rings of the wire sphere of `radius` around `center`: each a closed
/// loop of `SEGMENTS` points (the first not repeated).
pub fn sphere_rings(center: Vec3, radius: f32) -> Vec<Vec<Vec3>> {
    let mut rings = Vec::with_capacity(LATITUDES + MERIDIANS);
    let step = std::f32::consts::TAU / SEGMENTS as f32;
    for i in 1..=LATITUDES {
        // from near the south pole to near the north one
        let lat = std::f32::consts::PI * (i as f32 / (LATITUDES + 1) as f32 - 0.5);
        let (r, z) = (radius * lat.cos(), radius * lat.sin());
        rings.push(
            (0..SEGMENTS)
                .map(|k| {
                    let a = k as f32 * step;
                    center + Vec3::new(r * a.cos(), r * a.sin(), z)
                })
                .collect(),
        );
    }
    for m in 0..MERIDIANS {
        let heading = std::f32::consts::PI * m as f32 / MERIDIANS as f32;
        let (hx, hy) = (heading.cos(), heading.sin());
        rings.push(
            (0..SEGMENTS)
                .map(|k| {
                    let a = k as f32 * step;
                    center + Vec3::new(radius * a.cos() * hx, radius * a.cos() * hy, radius * a.sin())
                })
                .collect(),
        );
    }
    rings
}

/// The whisper, say and shout spheres around our avatar, with the region's
/// ranges (LFSimFeatureHandler).
pub fn chat_spheres(ctx: &egui::Context, world: &World, proj: &Projector) {
    let painter = super::hud::world_painter(ctx);
    let ranges = crate::world::radar::chat_ranges();
    let k = super::colors::get();
    let center = world.agent.position;
    for (radius, color) in [
        (ranges.whisper, k.map_whisper),
        (ranges.say, k.map_say),
        (ranges.shout, k.map_shout),
    ] {
        let stroke = Stroke::new(1.0, super::colors::c(color).gamma_multiply(0.35));
        for ring in sphere_rings(center, radius) {
            for (i, a) in ring.iter().enumerate() {
                let b = ring[(i + 1) % ring.len()];
                if let Some(segment) = super::minimap::beacon_segment(proj.view_proj * a.extend(1.0), proj.view_proj * b.extend(1.0), proj)
                {
                    painter.line_segment(segment, stroke);
                }
            }
        }
    }
}

/// The residents within `range` of our avatar (ExodusMouselookIFFRange)
/// that are out of view: an arrow at the edge of the screen toward each,
/// in their colour of the mini-map (contact set, mark).
pub fn mouselook_markers(ctx: &egui::Context, p: &Palette, world: &World, proj: &Projector, range: f32) {
    // RLVa's @shownearby hides the radar: these too
    if world.rlva.has("shownearby") {
        return;
    }
    let painter = super::hud::world_painter(ctx);
    let inner = ctx.content_rect().shrink2(vec2(24.0, 24.0));
    let me = world.agent.position;
    for s in crate::world::radar::sightings(world) {
        if s.position.distance(me) > range {
            continue;
        }
        let clip = proj.view_proj * s.position.extend(1.0);
        // drawMarker with is_iff: nothing for one in view
        let Some((at, false, direction)) = super::minimap::beacon_marker(clip, inner, proj) else {
            continue;
        };
        let name = world.social.name_of(&s.id);
        let color = super::colors::map_color(world, &s.id, &name);
        let color = world.avatar_marks.get(&s.id).map_or(color, |[r, g, b]| Color32::from_rgb(r, g, b));
        arrow(&painter, p, at, direction, color);
    }
}

/// The arrow of a marker, turned toward `direction`, with a dark outline.
fn arrow(painter: &egui::Painter, p: &Palette, at: Pos2, direction: egui::Vec2, color: Color32) {
    let Some(texture) = super::icons::global("arrow-right") else {
        return;
    };
    for (size, color) in [(26.0, p.bar), (22.0, color)] {
        let mut mesh = egui::Mesh::with_texture(texture.id());
        mesh.add_rect_with_uv(
            Rect::from_center_size(at, vec2(size, size)),
            Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
            color,
        );
        mesh.rotate(egui::emath::Rot2::from_angle(direction.angle()), at);
        painter.add(mesh);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_point_of_a_sphere_s_rings_lies_on_it() {
        let center = Vec3::new(128.0, 64.0, 22.0);
        let rings = sphere_rings(center, 20.0);
        assert_eq!(rings.len(), LATITUDES + MERIDIANS);
        for ring in &rings {
            assert_eq!(ring.len(), SEGMENTS);
            for point in ring {
                assert!((point.distance(center) - 20.0).abs() < 1e-3);
            }
        }
        // the middle ring of latitude is the equator, at the avatar's height
        assert!(rings[LATITUDES / 2].iter().all(|p| (p.z - center.z).abs() < 1e-3));
    }
}
