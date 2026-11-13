//! Routes and page paths: the game's RRUI navigation (Router, BrowserModel, WatchModelController) as far as the menu
//! needs it.
//!
//! A route is a view GUID (16 raw bytes, kept as hex in the bytes' order); a URI is a list of routes, outermost page
//! first, each one shown inside the `RRUI.Data.Router` element of the page before it (the `Navigation_*` pages are such
//! hosts, and so is the watch's root screen). Here a page is named by its *path*: the addresses of those pages joined
//! by [`SEP`], e.g. `(RRUIRootScreen) > (Navigation_People) > (Friends)`.
//!
//! The game's route -> prefab table (`RoutesConfig` assets) is in the scrambled resources, so [`ROUTES`] is made by
//! hand and INFERRED: route names come from the root screen's fields (spec appendix A), the routers' prefill routes and
//! the labels of the buttons that link to them, matched to the catalog's page addresses by name.
use unity_assets::Value;

use super::prefab::{Element, RectRules};

/// The watch: the root screen with its navigation bar, routed pages inside its Router.
pub const ROOT: &str = "(RRUIRootScreen)";
/// Between the parts of a page path.
pub const SEP: &str = " > ";
/// The watch's default URI (the root screen's BrowserModel.defaultUri: home, explore).
pub const HOME: &str = "(RRUIRootScreen) > (Navigation_Home) > (Explore)";

/// Route GUID -> page address (settings routes are in `menu::ROUTES`). All INFERRED as said above.
pub const ROUTES: &[(&str, &str)] = &[
    // home (root defaultUri) and its prefill
    ("22e12d326a573844bd7b875f9bc30788", "(Navigation_Home)"),
    ("f3ab5c04d01f0849a33a31d9a5d3f8d4", "(Explore)"),
    // play (nav bar PlayTabImpl), grid, search, categories
    ("73c4dce79edfc348a5e9f6d43d5d2f1d", "(PlayHighlight2.0)"),
    ("395cf87df3124b4a874d374e8a9a14b1", "(PlayGridView)"),
    ("2539469bb192334f905964b1f6f6fc2e", "(PlaySearch2.0)"),
    ("7d7ae79118477f45bfd51cc2237340b5", "(PlayHighlight_CategorySelect)"),
    // this room (nav bar CurrentRoomWatchTabsImpl) and its chips
    ("b24747dc6536514cad99f9b380b187f9", "(Navigation_ThisRoom)"),
    ("8dd1d2438e2bfc4399f8a6c6a169bb14", "(ThisRoom_RoomDetails2.0)"),
    // my profile (nav bar ProfileTabImpl), profile pages
    ("8ce6146f69ae4144a046ac6714c3dc4e", "(Navigation_MyProfile)"),
    ("6905e3ae030c7e44bba99f1eae53c721", "(PlayerProfile)_MyProfileVariant"),
    ("a4012c9a9f1a544495fdb9484bf81686", "(PlayerProfile)"),
    ("e049382e6b9bae47be8509ab6789cefb", "(PlayerProgressionMain)"),
    ("a4a81fadd8331f428f39f38d658e75f9", "(SavedOutfits)"),
    ("226afab02e066b43b42ca245fa8ee678", "(Portfolio2.0)"),
    ("cf5c7dc26bef42439016c0b3afb839c6", "(ReactionCustomizer)"),
    // people (nav bar) and its tabs: Friends, People, Party, Subscriptions
    ("99e6f57814e0424f95833105ced52059", "(Navigation_People)"),
    ("294bde26843e0345817c010516c56fd6", "(Friends)"),
    ("050e28a267d4b44a92937aaafe8c573f", "(People)"),
    ("f23e70b026a143458a471fde02480790", "(Party)"),
    ("e394936216591440afa5b8ba2f9420de", "(Subscriptions)"),
    ("fed702c474cea64699e4db37b5d1314f", "(PeopleSearch)"),
    // backpack: the host (its router prefills Tools) and the chips of its pages
    ("937bbad26ebf6e4cba4b4d66921996e8", "(Navigation_Backpack)"),
    ("63f7220a70bcf64f96ffb9290005a47e", "(Backpack_Recent_Page)Variant"),
    ("10c91564374d3745bb7cccb2fe732b6e", "(Backpack_Tools_Page)Variant"),
    ("7c9abf434492704589701c9fc6e61a26", "(Backpack_Skins_Page)Variant"),
    ("0fce8e1e6a09c649868da41b9adc6805", "(Backpack_Other_Page)Variant"),
    ("a703be034028f544876fa9ee01a2e599", "(Backpack_Games_Page)Variant"),
    ("23b685042fb51748830842176e5450b2", "(Backpack_Food_Page)Variant"),
    ("cec8e8670652c742a05e9bc7e3927c89", "(Backpack_Room_Page)Variant"),
    ("1e3cef5e3e95c84c878b3684ed34f332", "(Backpack_RoomInventory_Page)Variant"),
    // chat and notifications (the nav bar's Chat and Noti buttons)
    ("e9e03d80c6635a44b9e7f99939be6055", "(ChatPage)"),
    ("169c659f051dc04e94eeaf7e242c509c", "(WatchNotificationBasePage)"),
    // store (root prefill) and create (nav bar) with their routers' prefills
    ("dbc0cb0604b73f40b99feb0333826f93", "(Navigation_Store)"),
    ("0627384e8cd72e4e9568bed91906a69f", "(StoreCategorized)"),
    ("3b6bbc94aea59848bd1ec13694a733ac", "(Navigation_Create)"),
    ("d9c785d6366d1d458d80a868f923e6df", "(CreateMyRooms)"),
    ("89a7a30abe19804b8ef20b2ac39660d4", "(Page_Create_MyDorms)"),
    ("0704b9875680c346824bb9605601c905", "(CreateCreatorHub)"),
    // customize (nav bar CustomizeTabsImpl)
    ("beb982c7570c044991d1f1ebc2197366", "(AvatarCustomization_Root_WatchFlow)"),
];

/// The page a route opens, from `ROUTES` or `more` (the settings routes).
pub fn page_of(route: &str, more: &[(&'static str, &'static str)]) -> Option<&'static str> {
    ROUTES.iter().chain(more).find(|(r, _)| *r == route).map(|(_, p)| *p)
}

pub fn parts(path: &str) -> Vec<&str> {
    path.split(SEP).collect()
}

/// The innermost page of a path.
pub fn leaf(path: &str) -> &str {
    path.rsplit(SEP).next().unwrap_or(path)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The routes an impl links to: its `uri`'s or `defaultUri`'s, or its one `route`.
pub fn uri(d: &Value) -> Vec<String> {
    let guid = |r: &Value| r.path("viewGuid/bytes").and_then(Value::as_bytes).map(hex);
    if let Some(a) = d.path("uri/route").or_else(|| d.path("defaultUri/route")).and_then(Value::as_array) {
        return a.iter().filter_map(guid).collect();
    }
    d.get("route").and_then(guid).into_iter().collect()
}

/// The path a link opens from the page at `current`. `absolute` (the watch's own tabs, and URIs of two or more
/// routes): the URI is the whole path inside the watch (or the whole path outside it); otherwise the one route
/// replaces the innermost page, as a nested Router handles the depth it is at (INFERRED from `GetRouteForDepth`).
pub fn target(current: &str, routes: &[&str], absolute: bool) -> String {
    let mut p = parts(current);
    if absolute || routes.len() > 1 || p.len() < 2 {
        p.truncate(usize::from(p[0] == ROOT));
    } else {
        p.pop();
    }
    p.extend(routes);
    p.join(SEP)
}

/// Whether a link's button shows as the current one: a tab when its first page is the one at that depth of the path,
/// another link when the path is its page or inside it.
pub fn current(current: &str, target: &str, routes: usize, tab: bool) -> bool {
    let (c, t) = (parts(current), parts(target));
    if tab {
        let i = t.len().saturating_sub(routes);
        return t.get(i).is_some() && c.get(i) == t.get(i);
    }
    c.starts_with(&t)
}

/// Puts each page of a path inside the Router of the page before it, filling the router's rect.
pub fn nest(mut pages: Vec<Element>) -> Option<Element> {
    let mut inner = pages.pop()?;
    while let Some(mut host) = pages.pop() {
        let slot = router(&mut host)?;
        inner.rect = RectRules { anchor_min: [0.0; 2], anchor_max: [1.0; 2], size_delta: [0.0; 2], ..RectRules::default() };
        inner.active = true;
        slot.children.push(inner);
        inner = host;
    }
    Some(inner)
}

/// The element holding a page's Router.
fn router(e: &mut Element) -> Option<&mut Element> {
    if e.components.iter().any(|c| c.class == "RRUI.Data.Router") {
        return Some(e);
    }
    e.children.iter_mut().find_map(router)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::prefab::Component;

    #[test]
    fn links_open_paths_inside_the_watch() {
        let friends = "(RRUIRootScreen) > (Navigation_People) > (Friends)";
        // a nav bar tab: the whole path inside the watch
        assert_eq!(target(friends, &["(PlayHighlight2.0)"], true), "(RRUIRootScreen) > (PlayHighlight2.0)");
        // a host's tab with a two-route URI
        assert_eq!(target(friends, &["(Navigation_People)", "(Party)"], false), "(RRUIRootScreen) > (Navigation_People) > (Party)");
        // a chip with one route: in place of the innermost page
        assert_eq!(target(friends, &["(People)"], false), "(RRUIRootScreen) > (Navigation_People) > (People)");
        // outside the watch a settings tab replaces the page, and the escape menu's settings button opens the page
        assert_eq!(target("(Watch_Settings_Page_Controls_Tab)", &["(Watch_Settings_Page_Audio_Tab)"], false), "(Watch_Settings_Page_Audio_Tab)");
        assert_eq!(target("(EscapeMenu_standalone)", &["(Watch_Settings_Page_Controls_Tab)"], true), "(Watch_Settings_Page_Controls_Tab)");
        assert_eq!(leaf(friends), "(Friends)");
        assert!(current(friends, "(RRUIRootScreen) > (Navigation_People) > (People)", 2, true));
        assert!(!current(friends, "(RRUIRootScreen) > (Navigation_Home) > (Explore)", 2, true));
        assert!(current(friends, friends, 2, false));
        assert!(!current(friends, "(RRUIRootScreen) > (Navigation_People) > (Party)", 2, false));
        assert_eq!(page_of("294bde26843e0345817c010516c56fd6", &[]), Some("(Friends)"));
    }

    #[test]
    fn a_page_goes_inside_its_hosts_router() {
        let el = |name: &str, router: bool, children: Vec<Element>| Element {
            name: name.into(), active: true, id: 0, transform: 0, rect: RectRules::default(), children,
            components: if router { vec![Component { class: "RRUI.Data.Router".into(), value: Value::Bool(true), file: None, id: 1, enabled: true, themed: Default::default() }] } else { vec![] },
        };
        let host = el("host", false, vec![el("bar", false, vec![]), el("Content", true, vec![])]);
        let page = nest(vec![host, el("inner", false, vec![])]).unwrap();
        let slot = &page.children[1];
        assert_eq!(slot.children[0].name, "inner");
        assert_eq!((slot.children[0].rect.anchor_min, slot.children[0].rect.anchor_max), ([0.0; 2], [1.0; 2]));
        assert!(nest(vec![el("no router", false, vec![]), el("inner", false, vec![])]).is_none());
    }
}
