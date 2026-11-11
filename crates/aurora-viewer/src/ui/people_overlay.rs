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
//! (MapWhisperRingColor, MapChatRingColor, MapShoutRingColor), where the
//! Legacy Viewer fills translucent spheres: a wire hides less of a scene.
//! Like the Legacy spheres, each segment is tested against the scene's
//! depth (`aurora_render::world_line_at`): a wall in front hides it.
//!
//! In mouselook, the resident nearest the crosshair's line within the
//! markers' range has their name and distance under it, in their colour
//! (FSMouselookCombatFeatures, LLViewerWindow::draw), unless RLVa hides
//! names.

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
                    // hidden where the scene stands in front of it
                    aurora_render::world_line_at(ctx, *a, b);
                    painter.line_segment(segment, stroke);
                }
            }
        }
    }
    aurora_render::world_label_at(ctx, None);
}

/// Half the width and half the height of the box around the crosshair's
/// line where a resident is "under the crosshair" (LLViewerWindow::draw:
/// 0.75 m across and 1.5 m up and down, in front of the camera).
const CROSSHAIR_HALF_WIDTH: f64 = 0.75;
const CROSSHAIR_HALF_HEIGHT: f64 = 1.5;
/// How far under the screen's centre the name is written
/// (ExodusMouselookTextOffsetY, -150).
const CROSSHAIR_TEXT_BELOW: f32 = 150.0;

/// The first of `candidates` (id, position) under the crosshair of a camera
/// at `eye` looking along `forward` (world up Z), with its distance; ourselves
/// excluded by the caller. The Legacy Viewer takes the first in its list:
/// here the nearest, which does not depend on the list's order.
pub fn crosshair_target(eye: Vec3, forward: Vec3, candidates: impl Iterator<Item = (uuid::Uuid, Vec3)>) -> Option<(uuid::Uuid, f32)> {
    let forward = forward.normalize_or_zero().as_dvec3();
    if forward == glam::DVec3::ZERO {
        return None;
    }
    let left = glam::DVec3::Z.cross(forward).normalize_or(glam::DVec3::Y);
    let up = forward.cross(left);
    candidates
        .filter_map(|(id, at)| {
            let d = (at - eye).as_dvec3();
            let (x, y, z) = (-d.dot(left), d.dot(up), d.dot(forward));
            (x.abs() < CROSSHAIR_HALF_WIDTH && y.abs() < CROSSHAIR_HALF_HEIGHT && z > 0.0).then_some((id, d.length() as f32))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// "Name, 12.34m" under the crosshair (see the module), centred.
pub fn crosshair_name(ctx: &egui::Context, p: &Palette, world: &World, eye: Vec3, forward: Vec3, range: f32) {
    if world.rlva.has("shownames") || world.rlva.has("shownearby") {
        return;
    }
    let me = world.agent.position;
    let near = crate::world::radar::sightings(world)
        .into_iter()
        .filter(|s| s.id != world.agent_id && s.position.distance(me) <= range)
        .map(|s| (s.id, s.position));
    let Some((id, distance)) = crosshair_target(eye, forward, near) else {
        return;
    };
    let name = world
        .social
        .avatar_names
        .complete(&id)
        .unwrap_or_else(|| aurora_i18n::tr!("l2s-crosshair-unknown").to_owned());
    let color = super::colors::map_color(world, &id, &name);
    let color = world.avatar_marks.get(&id).map_or(color, |[r, g, b]| Color32::from_rgb(r, g, b));
    let color = super::colors::shadow_red(world, &id).unwrap_or(color);
    let text = aurora_i18n::tr!(
        "l2s-crosshair-name",
        name = name.as_str(),
        meters = aurora_i18n::number(f64::from(distance), 2)
    );
    let at = ctx.content_rect().center() + vec2(0.0, CROSSHAIR_TEXT_BELOW);
    let painter = super::hud::world_painter(ctx);
    let font = egui::FontId::new(14.0, super::fonts::semibold(ctx));
    // a soft shadow under the letters (DROP_SHADOW_SOFT)
    painter.text(at + vec2(1.0, 1.0), egui::Align2::CENTER_TOP, &text, font.clone(), p.bar);
    painter.text(at, egui::Align2::CENTER_TOP, &text, font, color);
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
        let color = super::colors::shadow_red(world, &s.id).unwrap_or(color);
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

    #[test]
    fn the_crosshair_takes_the_nearest_resident_on_its_line() {
        let eye = Vec3::new(100.0, 100.0, 30.0);
        let forward = Vec3::X;
        let (a, b, c, d) = (
            uuid::Uuid::from_u128(1),
            uuid::Uuid::from_u128(2),
            uuid::Uuid::from_u128(3),
            uuid::Uuid::from_u128(4),
        );
        let list = [
            (a, Vec3::new(130.0, 100.5, 31.0)),
            // nearer, still within 0.75 m across and 1.5 m up and down
            (b, Vec3::new(110.0, 99.4, 28.6)),
            // too far to the side, and behind the camera
            (c, Vec3::new(105.0, 101.0, 30.0)),
            (d, Vec3::new(90.0, 100.0, 30.0)),
        ];
        let (id, distance) = crosshair_target(eye, forward, list.into_iter()).expect("one");
        assert_eq!(id, b);
        assert!((distance - Vec3::new(10.0, -0.6, -1.4).length()).abs() < 1e-3);
        assert_eq!(crosshair_target(eye, forward, list[2..].iter().copied()), None);
        // looking up: the box turns with the camera
        let up = Vec3::new(1.0, 0.0, 1.0);
        assert_eq!(
            crosshair_target(eye, up, [(a, Vec3::new(110.0, 100.0, 40.0))].into_iter()).map(|t| t.0),
            Some(a)
        );
    }
}
