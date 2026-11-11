//! The Second Life web guides of the World menu, shown in Aurora's built-in
//! browser (`web_browser.rs`): the Avatar Welcome Pack
//! (LLFloaterAvatarWelcomePack, AvatarWelcomePack) and My Linden Home
//! (LLWorldLindenHome, indra/newview/llviewermenu.cpp). Addresses and
//! choices from the Legacy Viewer's settings.xml and strings.xml (originally
//! LGPL 2.1); [GRID_LOWERCASE] is the grid's id (LLWeb's substitution).
//! Second Life grids only, as the Legacy Viewer's GridCheck "secondlife":
//! the pages are Linden Lab's. The Destination Guide is its own window
//! (`ui/destinations.rs`).

use crate::settings::GridChoice;

/// A guide of the World menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guide {
    WelcomePack,
    LindenHome,
}

/// ProductSKU of a Linden Home (LINDEN_HOMES_SKU, llfloaterlandholdings.cpp).
pub const LINDEN_HOME_SKU: &str = "131";

const WELCOME_PACK: &str = "https://lecs-viewer-web-components.s3.amazonaws.com/v3.0/[GRID_LOWERCASE]/vawp/index.html";
/// lindenhomes_my_home_url and lindenhomes_get_home_url.
const MY_LINDEN_HOME: &str = "https://land.secondlife.com/en-Us/lindenhomes/my-home.php";
const GET_LINDEN_HOME: &str = "https://secondlife.com/land/lindenhomes/member.php";

/// The page of a guide on this grid; None outside Second Life.
/// `has_linden_home`: our land holds a Linden Home (sHasLindenHome).
pub fn url(guide: Guide, grid: &GridChoice, has_linden_home: bool) -> Option<String> {
    let id = match grid {
        GridChoice::SecondLife => "agni",
        GridChoice::SecondLifeBeta => "aditi",
        GridChoice::Custom => return None,
    };
    Some(match guide {
        Guide::WelcomePack => WELCOME_PACK.replace("[GRID_LOWERCASE]", id),
        Guide::LindenHome if has_linden_home => MY_LINDEN_HOME.to_owned(),
        Guide::LindenHome => GET_LINDEN_HOME.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guides_name_the_grid_and_exist_on_second_life_only() {
        assert_eq!(
            url(Guide::WelcomePack, &GridChoice::SecondLifeBeta, false).as_deref(),
            Some("https://lecs-viewer-web-components.s3.amazonaws.com/v3.0/aditi/vawp/index.html")
        );
        assert_eq!(url(Guide::WelcomePack, &GridChoice::Custom, false), None);
    }

    #[test]
    fn my_linden_home_depends_on_owning_one() {
        assert_eq!(
            url(Guide::LindenHome, &GridChoice::SecondLife, true).as_deref(),
            Some(MY_LINDEN_HOME)
        );
        assert_eq!(
            url(Guide::LindenHome, &GridChoice::SecondLife, false).as_deref(),
            Some(GET_LINDEN_HOME)
        );
    }
}
