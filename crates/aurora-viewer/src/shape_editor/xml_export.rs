//! Our appearance written as an appearance XML file (`linden_genepool` 1.0),
//! the file `xml_import` and shape websites read (Me › Avatar health ›
//! Appearance to XML).
//!
//! Port of LLVOAvatar::dumpArchetypeXML grouped by wearable, with
//! dump_visual_param (indra/newview/llvoavatar.cpp, originally LGPL 2.1):
//! one `<!-- wearable -->` comment per wearable type, then its tweakable
//! parameters with their weight and its quantised value. Our own avatar
//! only.
//!
//! Differences, all stricter: no `<texture>` lines (the Legacy Viewer
//! writes them with a null id unless the user is a god), and none of the
//! debugging lines after the parameters (joints, attachment points, rigged
//! mesh offsets with their mesh ids), so that no asset id is written. Only
//! the parameters an appearance transmits are known here; the others are
//! not listed rather than written with a guessed weight.
use crate::scene::shape::Dumped;
use std::fmt::Write as _;

/// XML attribute text.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// F32_to_U8 (llquantize.h), as dump_visual_param writes it.
fn quantised(weight: f32, min: f32, max: f32) -> u8 {
    if max <= min || !max.is_finite() || !min.is_finite() || !weight.is_finite() {
        return 0;
    }
    ((weight.clamp(min, max) - min) / (max - min) * 255.0).round() as u8
}

/// The file's text for these parameters (`ShapeParams::dump`).
pub fn archetype_xml(rows: &[Dumped]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"US-ASCII\" standalone=\"yes\"?>\n<linden_genepool version=\"1.0\">\n\n\t<archetype name=\"???\">\n",
    );
    let mut wearable = None;
    for row in rows.iter().filter(|r| r.weight.is_finite()) {
        if wearable != Some(row.wearable) {
            wearable = Some(row.wearable);
            let name = crate::world::inventory::wearable::type_name(row.wearable).unwrap_or("invalid");
            let _ = writeln!(out, "\n\t\t<!-- wearable: {name} -->");
        }
        let _ = writeln!(
            out,
            "\t\t<param id=\"{}\" name=\"{}\" value=\"{:.3}\" u8=\"{}\" wearable=\"{}\" group=\"0\"/>",
            row.id,
            escaped(&row.name),
            row.weight,
            quantised(row.weight, row.min, row.max),
            crate::world::inventory::wearable::type_name(row.wearable).unwrap_or("invalid"),
        );
    }
    out.push_str("\t</archetype>\n\n</linden_genepool>\n");
    out
}

/// Ask where to save, then write `text`, on a worker thread. The answer
/// (the path, or None when cancelled, or the error) comes back on the
/// receiver.
pub fn save(ctx: &egui::Context, text: String) -> Option<std::sync::mpsc::Receiver<Result<Option<std::path::PathBuf>, String>>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    let spawned = std::thread::Builder::new().name("aurora-appearance-dump".into()).spawn(move || {
        let path = rfd::FileDialog::new()
            .set_title(aurora_i18n::tr!("b4-dump-title"))
            .add_filter(aurora_i18n::tr!("appearance-xml-file-type"), &["xml"])
            .set_file_name("appearance.xml")
            .save_file();
        let result = match path {
            None => Ok(None),
            Some(path) => std::fs::write(&path, text.as_bytes())
                .map(|_| Some(path))
                .map_err(|e| e.to_string()),
        };
        let _ = sender.send(result);
        ctx.request_repaint();
    });
    match spawned {
        Ok(_) => Some(receiver),
        Err(e) => {
            log::warn!("cannot start the appearance dump worker: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lad() -> &'static crate::scene::shape::ShapeParams {
        static LIBRARY: std::sync::OnceLock<crate::scene::avatar::AvatarLibrary> = std::sync::OnceLock::new();
        &LIBRARY.get_or_init(crate::scene::avatar::AvatarLibrary::load).shape_params
    }

    #[test]
    fn a_dump_reads_back_through_the_import_with_the_same_weights() {
        let lad = lad();
        // Height (33) high, the rest at its default
        let values = lad.values_with(&[(33, 1.5), (80, 1.0)]);
        let rows = lad.dump(&values);
        assert!(rows.iter().any(|r| r.id == 33 && r.wearable == 0));
        assert!(rows.windows(2).all(|w| (w[0].wearable, w[0].id) < (w[1].wearable, w[1].id)));
        let text = archetype_xml(&rows);
        assert!(text.starts_with("<?xml"));
        assert!(text.contains("<!-- wearable: shape -->"));
        assert!(!text.contains("<texture"), "no texture ids");
        let back = super::super::xml_import::parse(text.as_bytes()).expect("import");
        assert_eq!(back.errors, 0);
        assert_eq!(back.params.len(), rows.len());
        let mut shape = std::collections::BTreeMap::new();
        super::super::xml_import::apply(&back, 0, lad, &mut shape);
        let height = shape.get(&33).copied().expect("height");
        let dumped = rows.iter().find(|r| r.id == 33).expect("row").weight;
        assert!((height - dumped).abs() < 0.001, "{height} vs {dumped}");
        assert!(shape.get(&80).is_some_and(|male| *male > 0.5));
    }

    #[test]
    fn values_are_quantised_and_names_escaped() {
        assert_eq!(quantised(0.5, 0.0, 1.0), 128);
        assert_eq!(quantised(-3.0, -1.0, 1.0), 0);
        assert_eq!(quantised(f32::NAN, 0.0, 1.0), 0);
        assert_eq!(quantised(1.0, 1.0, 1.0), 0);
        assert_eq!(escaped("a<b & \"c\""), "a&lt;b &amp; &quot;c&quot;");
        let rows = [Dumped {
            id: 7,
            name: "x\"y".into(),
            wearable: 2,
            weight: 0.25,
            min: 0.0,
            max: 1.0,
        }];
        let text = archetype_xml(&rows);
        assert!(text.contains("<!-- wearable: hair -->"));
        assert!(
            text.contains(r#"<param id="7" name="x&quot;y" value="0.250" u8="64" wearable="hair" group="0"/>"#),
            "{text}"
        );
    }
}
