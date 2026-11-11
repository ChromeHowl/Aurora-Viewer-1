//! Collada 1.4.1 model of a linkset (the Legacy Viewer's "Save as ▸
//! Collada", ColladaExportFloater / DAESaver in daeexport.cpp, behaviour
//! only): one node per prim, its transform relative to the root's position,
//! one geometry per face with positions, normals and texture coordinates,
//! and one material per face carrying its colour. Textures are not written
//! (no picture files and no asset ids).

use glam::Mat4;
use std::fmt::Write;

/// One face of a prim, at unit scale (the node's transform scales it).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Face {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u16>,
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub name: String,
    pub transform: Mat4,
    pub faces: Vec<Face>,
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn floats(out: &mut String, values: impl Iterator<Item = f32>) {
    for (i, v) in values.enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let v = if v.is_finite() { v } else { 0.0 };
        let _ = write!(out, "{v}");
    }
}

fn source(out: &mut String, id: &str, data: &[f32], stride: usize, params: &[&str]) {
    let count = data.len() / stride.max(1);
    let _ = write!(out, "<source id=\"{id}\"><float_array id=\"{id}-array\" count=\"{}\">", data.len());
    floats(out, data.iter().copied());
    let _ = write!(
        out,
        "</float_array><technique_common><accessor source=\"#{id}-array\" count=\"{count}\" stride=\"{stride}\">"
    );
    for p in params {
        let _ = write!(out, "<param name=\"{p}\" type=\"float\"/>");
    }
    out.push_str("</accessor></technique_common></source>");
}

/// Whether a face can be written: whole triangles on vertices it has.
fn usable(f: &Face) -> bool {
    !f.positions.is_empty() && f.indices.len() >= 3 && f.indices.iter().all(|&i| (i as usize) < f.positions.len())
}

/// The Collada document of `nodes`.
pub fn write(nodes: &[Node], author: &str, tool: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str("<COLLADA xmlns=\"http://www.collada.org/2005/11/COLLADASchema\" version=\"1.4.1\">");
    let _ = write!(
        out,
        "<asset><contributor><author>{}</author><authoring_tool>{}</authoring_tool></contributor><unit name=\"meter\" meter=\"1\"/><up_axis>Z_UP</up_axis></asset>",
        escape(author),
        escape(tool)
    );
    let faces = || {
        nodes.iter().enumerate().flat_map(|(n, node)| {
            node.faces
                .iter()
                .enumerate()
                .filter(|(_, f)| usable(f))
                .map(move |(i, f)| (n, i, f))
        })
    };
    out.push_str("<library_effects>");
    for (n, i, f) in faces() {
        let _ = write!(
            out,
            "<effect id=\"effect-{n}-{i}\"><profile_COMMON><technique sid=\"common\"><phong><diffuse><color>"
        );
        floats(&mut out, f.color.iter().copied());
        out.push_str("</color></diffuse>");
        if f.color[3] < 1.0 {
            let _ = write!(out, "<transparency><float>{}</float></transparency>", f.color[3].clamp(0.0, 1.0));
        }
        out.push_str("</phong></technique></profile_COMMON></effect>");
    }
    out.push_str("</library_effects><library_materials>");
    for (n, i, _) in faces() {
        let _ = write!(
            out,
            "<material id=\"material-{n}-{i}\"><instance_effect url=\"#effect-{n}-{i}\"/></material>"
        );
    }
    out.push_str("</library_materials><library_geometries>");
    for (n, i, f) in faces() {
        let id = format!("geometry-{n}-{i}");
        let _ = write!(out, "<geometry id=\"{id}\"><mesh>");
        let positions: Vec<f32> = f.positions.iter().flatten().copied().collect();
        source(&mut out, &format!("{id}-positions"), &positions, 3, &["X", "Y", "Z"]);
        let normals = f.normals.len() == f.positions.len();
        let uvs = f.uvs.len() == f.positions.len();
        if normals {
            let data: Vec<f32> = f.normals.iter().flatten().copied().collect();
            source(&mut out, &format!("{id}-normals"), &data, 3, &["X", "Y", "Z"]);
        }
        if uvs {
            let data: Vec<f32> = f.uvs.iter().flatten().copied().collect();
            source(&mut out, &format!("{id}-uvs"), &data, 2, &["S", "T"]);
        }
        let _ = write!(
            out,
            "<vertices id=\"{id}-vertices\"><input semantic=\"POSITION\" source=\"#{id}-positions\"/></vertices>"
        );
        let triangles = f.indices.len() / 3;
        let _ = write!(out, "<triangles material=\"material-{n}-{i}\" count=\"{triangles}\">");
        let _ = write!(out, "<input semantic=\"VERTEX\" source=\"#{id}-vertices\" offset=\"0\"/>");
        if normals {
            let _ = write!(out, "<input semantic=\"NORMAL\" source=\"#{id}-normals\" offset=\"0\"/>");
        }
        if uvs {
            let _ = write!(out, "<input semantic=\"TEXCOORD\" source=\"#{id}-uvs\" offset=\"0\" set=\"0\"/>");
        }
        out.push_str("<p>");
        for (k, index) in f.indices.iter().take(triangles * 3).enumerate() {
            if k > 0 {
                out.push(' ');
            }
            let _ = write!(out, "{index}");
        }
        out.push_str("</p></triangles></mesh></geometry>");
    }
    out.push_str("</library_geometries><library_visual_scenes><visual_scene id=\"scene\" name=\"scene\">");
    for (n, node) in nodes.iter().enumerate() {
        let _ = write!(
            out,
            "<node id=\"node-{n}\" name=\"{}\"><matrix sid=\"transform\">",
            escape(&node.name)
        );
        // Collada matrices are written row by row
        floats(&mut out, node.transform.transpose().to_cols_array().into_iter());
        out.push_str("</matrix>");
        for (i, _) in node.faces.iter().enumerate().filter(|(_, f)| usable(f)) {
            let _ = write!(
                out,
                "<instance_geometry url=\"#geometry-{n}-{i}\"><bind_material><technique_common><instance_material symbol=\"material-{n}-{i}\" target=\"#material-{n}-{i}\"/></technique_common></bind_material></instance_geometry>"
            );
        }
        out.push_str("</node>");
    }
    out.push_str("</visual_scene></library_visual_scenes><scene><instance_visual_scene url=\"#scene\"/></scene></COLLADA>\n");
    out
}
