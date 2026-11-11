//! What a photo really holds of the effects the screen shows, told as it
//! is: an effect is "applied" only when the photo drew it the way the
//! screen does, "not applied" when the photo left it out, and "unverified"
//! when the photo drew it in parts that its effect may reach across. The
//! window lists the last two after a photo (and the log all three), never
//! claiming an effect the photo does not have.
//!
//! A photo drawn in one pass draws every effect as the screen does, sized
//! for its pixels (the depth of field's blur, the bloom's halo, the contact
//! shadows' reach, the ambient occlusion's radius in metres). Drawn in
//! parts (`snapshot::plan`), each part draws a band of `guard` pixels
//! around what it keeps for the effects that read the image around a pixel:
//! an effect that reaches farther than the band, or the ambient occlusion
//! without the whole photo's depth around the parts (`ContextPass`), may
//! differ at the seams. The interface and the HUDs are laid out for the
//! window and never drawn in parts; the looks only go in a photo whose
//! screen shows them (the photo window open).

/// An effect a photo may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Interface,
    Huds,
    Looks,
    Bloom,
    DepthOfField,
    AmbientOcclusion,
    ContactShadows,
}

/// Why an effect is not in the photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    /// The photo is drawn in parts.
    Parts,
    /// The screen does not show it.
    Screen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Applied,
    NotApplied(Left),
    /// Drawn, but it may differ where the parts meet.
    Unverified,
}

/// What a photo was drawn with. Reaches are in the pixels drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Inputs {
    /// The parts it was drawn in.
    pub parts: usize,
    /// The band each part draws around what it keeps.
    pub guard: u32,
    /// Asked (and possible: the window's proportions), and drawn.
    pub interface: Option<bool>,
    pub huds: Option<bool>,
    /// Looks chosen: shown on screen, and how far they read (None: across
    /// the whole photo).
    pub looks: Option<(bool, Option<u32>)>,
    /// On: how far each reads.
    pub bloom: Option<u32>,
    pub dof: Option<u32>,
    pub contact: Option<u32>,
    /// On: whether the whole photo's depth was drawn around the parts.
    pub ao: Option<bool>,
}

impl Inputs {
    /// The same photo drawn again in `parts` with a band of `guard` (a part
    /// came back black and the rest is drawn smaller): the interface and the
    /// HUDs, drawn in one pass only, are then left out.
    pub fn replanned(self, parts: usize, guard: u32) -> Inputs {
        let one = parts <= 1;
        Inputs {
            parts,
            guard,
            interface: self.interface.map(|drawn| drawn && one),
            huds: self.huds.map(|drawn| drawn && one),
            ..self
        }
    }
}

/// The effects switched on or asked, and what the photo holds of each.
pub fn report(i: &Inputs) -> Vec<(Effect, Status)> {
    let parts = i.parts > 1;
    let reaches = |reach: u32| {
        if parts && reach > i.guard {
            Status::Unverified
        } else {
            Status::Applied
        }
    };
    let drawn = |drawn: bool| {
        if drawn && !parts {
            Status::Applied
        } else {
            Status::NotApplied(Left::Parts)
        }
    };
    let mut out = Vec::new();
    if let Some(d) = i.interface {
        out.push((Effect::Interface, drawn(d)));
    }
    if let Some(d) = i.huds {
        out.push((Effect::Huds, drawn(d)));
    }
    if let Some((shown, reach)) = i.looks {
        let status = match reach {
            _ if !shown => Status::NotApplied(Left::Screen),
            None if parts => Status::Unverified,
            None => Status::Applied,
            Some(r) => reaches(r),
        };
        out.push((Effect::Looks, status));
    }
    for (effect, reach) in [
        (Effect::Bloom, i.bloom),
        (Effect::DepthOfField, i.dof),
        (Effect::ContactShadows, i.contact),
    ] {
        if let Some(r) = reach {
            out.push((effect, reaches(r)));
        }
    }
    if let Some(context) = i.ao {
        let status = if parts && !context { Status::Unverified } else { Status::Applied };
        out.push((Effect::AmbientOcclusion, status));
    }
    out
}

impl Effect {
    pub fn name(self) -> &'static str {
        match self {
            Effect::Interface => aurora_i18n::tr!("photo-interface"),
            Effect::Huds => aurora_i18n::tr!("photo-huds"),
            Effect::Looks => aurora_i18n::tr!("photo-looks"),
            Effect::Bloom => aurora_i18n::tr!("ss-effect-bloom"),
            Effect::DepthOfField => aurora_i18n::tr!("ss-effect-dof"),
            Effect::AmbientOcclusion => aurora_i18n::tr!("ss-effect-ao"),
            Effect::ContactShadows => aurora_i18n::tr!("ss-effect-contact"),
        }
    }
}

/// The lines the window shows: the effects the photo does not hold as the
/// screen does (none when it holds them all).
pub fn lines(report: &[(Effect, Status)]) -> Vec<String> {
    report
        .iter()
        .filter_map(|(effect, status)| {
            let name = effect.name();
            Some(match status {
                Status::Applied => return None,
                Status::NotApplied(Left::Parts) => aurora_i18n::tr!("ss-not-applied-parts", effect = name),
                Status::NotApplied(Left::Screen) => aurora_i18n::tr!("ss-not-applied-screen", effect = name),
                Status::Unverified => aurora_i18n::tr!("ss-unverified", effect = name),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_pass_applies_what_is_on() {
        let i = Inputs {
            parts: 1,
            guard: 0,
            interface: Some(true),
            huds: Some(true),
            looks: Some((true, None)),
            bloom: Some(5000),
            dof: Some(300),
            contact: Some(40),
            ao: Some(false),
        };
        let r = report(&i);
        assert_eq!(r.len(), 7);
        assert!(r.iter().all(|(_, s)| *s == Status::Applied), "{r:?}");
        assert!(lines(&r).is_empty(), "nothing to tell");
    }

    #[test]
    fn nothing_on_reports_nothing() {
        assert!(
            report(&Inputs {
                parts: 4,
                ..Default::default()
            })
            .is_empty()
        );
    }

    #[test]
    fn parts_leave_the_interface_and_the_huds_out() {
        let i = Inputs {
            parts: 4,
            interface: Some(true),
            huds: Some(true),
            ..Default::default()
        };
        assert_eq!(
            report(&i),
            vec![
                (Effect::Interface, Status::NotApplied(Left::Parts)),
                (Effect::Huds, Status::NotApplied(Left::Parts))
            ]
        );
        // drawn in one pass, they are in it
        let i = Inputs {
            parts: 1,
            interface: Some(true),
            ..Default::default()
        };
        assert_eq!(report(&i), vec![(Effect::Interface, Status::Applied)]);
    }

    #[test]
    fn an_effect_reaching_past_the_band_is_unverified() {
        let i = Inputs {
            parts: 9,
            guard: 512,
            bloom: Some(1402),
            dof: Some(512),
            contact: Some(100),
            ao: Some(true),
            ..Default::default()
        };
        assert_eq!(
            report(&i),
            vec![
                (Effect::Bloom, Status::Unverified),
                (Effect::DepthOfField, Status::Applied),
                (Effect::ContactShadows, Status::Applied),
                (Effect::AmbientOcclusion, Status::Applied),
            ]
        );
        let without_context = Inputs { ao: Some(false), ..i };
        assert!(report(&without_context).contains(&(Effect::AmbientOcclusion, Status::Unverified)));
    }

    #[test]
    fn looks_follow_the_screen_and_their_reach() {
        let base = Inputs {
            parts: 4,
            guard: 256,
            ..Default::default()
        };
        let looks = |shown, reach| {
            report(&Inputs {
                looks: Some((shown, reach)),
                ..base
            })[0]
                .1
        };
        assert_eq!(
            looks(false, Some(0)),
            Status::NotApplied(Left::Screen),
            "not on screen: not in the photo"
        );
        assert_eq!(looks(true, Some(40)), Status::Applied);
        assert_eq!(looks(true, Some(400)), Status::Unverified);
        assert_eq!(looks(true, None), Status::Unverified, "a lens effect across the whole photo");
        let single = Inputs { parts: 1, ..base };
        assert_eq!(
            report(&Inputs {
                looks: Some((true, None)),
                ..single
            })[0]
                .1,
            Status::Applied
        );
    }

    #[test]
    fn a_photo_drawn_again_in_parts_is_told_as_it_is_drawn() {
        let one = Inputs {
            parts: 1,
            guard: 0,
            interface: Some(true),
            huds: Some(true),
            bloom: Some(1402),
            ..Default::default()
        };
        assert!(report(&one).iter().all(|(_, s)| *s == Status::Applied));
        // a part came back black: drawn again in 4 parts with a smaller band
        let again = one.replanned(4, 512);
        assert_eq!(
            report(&again),
            vec![
                (Effect::Interface, Status::NotApplied(Left::Parts)),
                (Effect::Huds, Status::NotApplied(Left::Parts)),
                (Effect::Bloom, Status::Unverified),
            ]
        );
        // never back to "drawn" by another plan in one pass
        assert_eq!(again.replanned(1, 512).interface, Some(false));
    }

    #[test]
    fn the_lines_never_say_applied() {
        let r = vec![
            (Effect::Bloom, Status::Applied),
            (Effect::Huds, Status::NotApplied(Left::Parts)),
            (Effect::Looks, Status::NotApplied(Left::Screen)),
            (Effect::AmbientOcclusion, Status::Unverified),
        ];
        let l = lines(&r);
        assert_eq!(l.len(), 3);
        assert!(l.iter().all(|line| !line.contains(Effect::Bloom.name())));
    }
}
