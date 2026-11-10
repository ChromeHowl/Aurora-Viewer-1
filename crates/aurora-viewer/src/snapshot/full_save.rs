//! The Full save (an Aurora addition): one click writes three files of the
//! same instant, the world held still by the photo mode's freeze
//! (`app::snapshot`, `photo_freeze_world`) from the first to the last:
//! the photo, `<name>_cutout.png` (the isolation chosen, our own avatar
//! when none is, on a transparent background) and `<name>_background`
//! (the scene without its avatars, [`isolate::Mode::Background`]). Each is
//! drawn as any photo is, one after the other, the next once the previous
//! is written: never more than one large photo in memory.

use super::isolate::{self, Isolation};
use super::{FileFormat, file_name};
use std::path::{Path, PathBuf};

/// The three files, in the order they are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Photo,
    Cutout,
    Background,
}

impl Part {
    pub const ALL: [Part; 3] = [Part::Photo, Part::Cutout, Part::Background];

    /// The isolation it is drawn with, given the one the user had chosen
    /// (`mode`, and whether something is chosen): the mode and whether the
    /// background is left transparent.
    pub fn isolation(self, mode: isolate::Mode, chosen: bool) -> (isolate::Mode, bool) {
        match self {
            Part::Photo => (isolate::Mode::Off, false),
            Part::Cutout => match mode {
                isolate::Mode::Chosen if chosen => (isolate::Mode::Chosen, true),
                _ => (isolate::Mode::Own, true),
            },
            Part::Background => (isolate::Mode::Background, false),
        }
    }
}

/// The paths of the three files in `folder`: the photo named as any photo
/// is (`free_path`), the others after it; the first number at which none
/// of the three exists (`exists`), so that nothing is written over.
pub fn paths(folder: &Path, base: &str, stamp: Option<&str>, format: FileFormat, exists: impl Fn(&Path) -> bool) -> [PathBuf; 3] {
    let mut n = 1;
    loop {
        let photo = folder.join(file_name(base, stamp, n, format));
        let stem = photo.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let all = [
            photo.clone(),
            folder.join(format!("{stem}_cutout.{}", FileFormat::Png.extension())),
            folder.join(format!("{stem}_background.{}", format.extension())),
        ];
        if n >= 9999 || !all.iter().any(|p| exists(p)) {
            return all;
        }
        n += 1;
    }
}

/// A Full save under way.
#[derive(Debug, Clone)]
pub struct FullSave {
    pub paths: [PathBuf; 3],
    /// The part being drawn or written.
    pub next: usize,
    /// It was started (its photo asked); the next one starts once it is
    /// written.
    pub started: bool,
    /// What it changes, given back at its end: the isolation's mode, its
    /// transparent background, the freeze of the world.
    pub mode: isolate::Mode,
    pub transparent: bool,
    pub frozen: bool,
}

impl FullSave {
    pub fn new(paths: [PathBuf; 3], iso: &Isolation, frozen: bool) -> Self {
        FullSave {
            paths,
            next: 0,
            started: false,
            mode: iso.mode,
            transparent: iso.transparent,
            frozen,
        }
    }

    pub fn part(&self) -> Option<Part> {
        Part::ALL.get(self.next).copied()
    }

    /// The isolation for the part being drawn set on `iso`, and its path.
    pub fn apply(&mut self, iso: &mut Isolation) -> Option<PathBuf> {
        let part = self.part()?;
        let (mode, transparent) = part.isolation(self.mode, !iso.chosen().is_empty());
        iso.set_mode(mode);
        iso.set_transparent(transparent);
        self.started = true;
        Some(self.paths[self.next].clone())
    }

    /// The user's isolation given back.
    pub fn restore(&self, iso: &mut Isolation) {
        iso.set_mode(self.mode);
        iso.set_transparent(self.transparent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use isolate::Mode;

    #[test]
    fn the_three_files_are_named_after_the_photo_and_never_written_over() {
        let folder = Path::new("photos");
        let [photo, cutout, background] = paths(folder, "Snap", None, FileFormat::Jpeg, |_| false);
        assert_eq!(photo, folder.join("Snap_001.jpg"));
        assert_eq!(cutout, folder.join("Snap_001_cutout.png"));
        assert_eq!(background, folder.join("Snap_001_background.jpg"));
        // a cut-out of number 1 already there: number 2 for all three
        let taken = folder.join("Snap_001_cutout.png");
        let [photo, ..] = paths(folder, "Snap", None, FileFormat::Png, |p| p == taken);
        assert_eq!(photo, folder.join("Snap_002.png"));
    }

    #[test]
    fn each_part_is_drawn_with_its_isolation_and_the_users_comes_back() {
        assert_eq!(Part::Photo.isolation(Mode::Chosen, true), (Mode::Off, false));
        assert_eq!(Part::Cutout.isolation(Mode::Chosen, true), (Mode::Chosen, true));
        assert_eq!(
            Part::Cutout.isolation(Mode::Chosen, false),
            (Mode::Own, true),
            "nothing chosen: our avatar"
        );
        assert_eq!(Part::Cutout.isolation(Mode::Off, false), (Mode::Own, true));
        assert_eq!(Part::Background.isolation(Mode::Own, false), (Mode::Background, false));

        let mut iso = Isolation::default();
        iso.set_open(true);
        iso.set_transparent(false);
        let mut full = FullSave::new(paths(Path::new("p"), "S", None, FileFormat::Png, |_| false), &iso, false);
        let mut seen = Vec::new();
        while full.part().is_some() {
            let path = full.apply(&mut iso).expect("a part");
            seen.push((iso.mode, iso.empty_background(), path.file_name().map(|n| n.to_owned())));
            full.next += 1;
        }
        assert_eq!(
            seen.iter().map(|s| (s.0, s.1)).collect::<Vec<_>>(),
            [(Mode::Off, false), (Mode::Own, true), (Mode::Background, false)]
        );
        full.restore(&mut iso);
        assert_eq!((iso.mode, iso.transparent), (Mode::Off, false));
    }
}
