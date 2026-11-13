//! The escape menu and the pages it opens. The pages are the game's prefabs; what the game's controller
//! implementations (`XModelController/YImpl`, see `prefab::Element::impls`) do is done here by class name: show or
//! hide parts, show settings values, list accounts, and turn clicks on the selectables they hold into [`Action`]s.
//! A [`Router`] keeps pages open over each other, as the game's BrowserModel history does; a page is a path of pages
//! nested in each other's Routers (see [`routes`]), so any routed page prefab can be opened, in the watch or alone.
//!
//! Behaviour is INFERRED from class and field names and the serialized data (no method bodies read): e.g. a
//! `HideIfBackStackEmptyImpl` hides its Hideable while there is no page to go back to, a `LinkButtonImpl` goes to its
//! route, a `SettingsModelController/XToggleImpl` flips setting X and shows it on its ToggleTheme.
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;

use unity_assets::{PPtr, Value};

use super::paint::{self, Drawn, Looks};
use super::prefab::{self, Element, Pages, TEMPLATE};
use super::rooms::{self, RoomInfo};
use super::routes;
use super::theme::Theme;
use crate::room::Library;
use crate::settings::{Settings, RESOLUTIONS};

/// The avatar-item card prefab that fills the Customize grids.
const ITEM_CARD: &str = "Assets/RRUI/Prefabs/WatchMenu/Components/Cards/(AvatarCustomizationItemCard).prefab";
pub const ESCAPE: &str = "(EscapeMenu_standalone)";
pub const SWITCH_ACCOUNT: &str = "(SwitchAccount)";
pub const LOGIN: &str = "(Login)";
/// The signed-in title screen (the real client's front door): REC ROOM logo, the handle, a hideable friends grid and
/// the big Play button, over the posed avatar.
pub const TITLE_CACHED: &str = "(TitleScreenCachedAccount)";
pub const CONTROLS: &str = "(Watch_Settings_Page_Controls_Tab)";
pub const VISUALS: &str = "(Watch_Settings_Page_Visuals_Tab)";
/// The rooms browser: the watch's Play grid (a card per room).
pub const ROOMS: &str = "(RRUIRootScreen) > (PlayGridView)";
/// The backpack in the watch (its router shows the Tools page first).
pub const BACKPACK: &str = "(RRUIRootScreen) > (Navigation_Backpack)";
/// Rows the client adds to the escape menu, under its Settings row: label, page path. The game reaches these pages
/// from the watch (AppNav); the screen-mode client has no watch button of its own.
const ESCAPE_ROWS: &[(&str, &str)] = &[("Watch", routes::HOME), ("Rooms", ROOMS), ("Backpack", BACKPACK)];

/// Route GUIDs (the 16 raw bytes, as hex) and their pages. INFERRED: each settings route is matched to the tab page
/// named like the label of the chip that links to it; the game's route table is in the scrambled resources.
const ROUTES: &[(&str, &str)] = &[
    ("4c33ae70f8964d4eb4ba4e5dc16665d6", CONTROLS),
    ("a5cbbefc9573c645bb06db752e63253d", "(Watch_Settings_Page_Experience_Tab)"),
    ("48c801b81c12124bb45db9c41f94b20a", VISUALS),
    ("041657165fbb9f41a441537f35c1d6c5", "(Watch_Settings_Page_Audio_Tab)"),
    ("065a5b072d59fb4aa83d3444ea895cd3", "(Watch_Settings_Page_Comfort_Tab)"),
    ("12b69c8b03d8404bbb4e08fafe1ff566", "(Watch_Settings_Page_Export_Tab)"),
    ("15414df7960bc44781cf3af00421eafa", "(Watch_Settings_Page_Misc_Tab)"),
    ("eb3a5a2933ace243b78a5080107db884", "(Watch_Settings_Page_Notifications_Tab)"),
    ("af20ad2e3766324ca94e0e817c1e4081", "(Watch_Settings_Page_DevOnly_Tab)"),
];

/// Every page the menu can show, for loading up front.
pub fn pages() -> Vec<&'static str> {
    let mut v = vec![ESCAPE, SWITCH_ACCOUNT, LOGIN];
    v.extend(ROUTES.iter().map(|(_, p)| *p));
    v
}

fn page_of(route: &str) -> Option<&'static str> {
    routes::page_of(route, ROUTES)
}

/// The first route a page's Router shows before it is told otherwise (its first prefill route).
fn prefill(e: &Element) -> Option<String> {
    let router = e.walk().into_iter().find_map(|(_, k)| k.component("RRUI.Data.Router"))?;
    let first = router.value.get("prefillRoutes").and_then(Value::as_array)?.first()?;
    first.path("viewGuid/bytes").and_then(Value::as_bytes).map(hex)
}

/// Pages open over each other: the bottom one is the escape menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Router {
    pages: Vec<String>,
}

impl Router {
    pub fn open(&mut self, page: &str) {
        self.pages.push(page.to_string());
    }

    /// A tab of the page on top (a page group): replaces it, so Back skips the tabs seen.
    pub fn tab(&mut self, page: &str) {
        match self.pages.last_mut() {
            Some(top) => *top = page.to_string(),
            None => self.open(page),
        }
    }

    /// Back one page; false when there was none to go back to.
    pub fn back(&mut self) -> bool {
        self.pages.len() > 1 && self.pages.pop().is_some()
    }

    pub fn close(&mut self) {
        self.pages.clear();
    }

    pub fn top(&self) -> Option<&str> {
        self.pages.last().map(String::as_str)
    }

    /// Pages under the top one.
    pub fn back_stack(&self) -> usize {
        self.pages.len().saturating_sub(1)
    }
}

/// What a click on a page does.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Back,
    Close,
    /// open a page; `tab`: in place of the top one
    Go { page: String, tab: bool },
    SwitchAccount,
    Quit,
    /// flip a switch setting
    Toggle(String),
    /// set a setting to one of its values (a radio button, or an option of a choice dialog)
    Choose(String, f64),
    /// open the choice dialog of a dropdown setting
    Dropdown(String),
    /// close the choice dialog
    CloseDialog,
    /// a slider setting, set by dragging
    Slider(String),
    /// log in as this account
    Account(String),
    /// the name field of the login page, which takes typing once clicked
    Field,
    /// log in with the typed name
    Login,
    /// leave this room and join the named one (a room card): its name and scene (empty when the server names none)
    GoToRoom(String, String),
    /// wear a wardrobe item from a Customize grid: the game's OutfitType number and the item's style name
    WearItem(i32, String),
}

/// What a page shows besides its prefab.
pub struct Ctx<'a> {
    pub page: &'a str,
    pub back_stack: usize,
    pub settings: &'a Settings,
    /// the account logged in now
    pub account: &'a str,
    /// the login page's name field and whether it has the keyboard
    pub typed: &'a str,
    pub focused: bool,
    /// scroll offsets by ScrollRect component id, in canvas units down from the top
    pub scroll: &'a HashMap<i64, f32>,
    /// the dropdown setting whose choice dialog is open
    pub choosing: Option<&'a str>,
    /// the server's rooms, for room lists
    pub rooms: &'a [RoomInfo],
    /// the loaded (AvatarCustomizationItemCard) template, to fill the avatar-item grids
    pub item_card: Option<&'a Element>,
}

/// A page made ready to lay out.
pub struct Built {
    pub page: Element,
    pub actions: HashMap<i64, Action>,
    /// ScrollRect component id, content and viewport RectTransform ids
    pub scrolls: Vec<(i64, i64, i64)>,
    /// runtime-assigned graphics: (Image component id, icon PNG file name) — e.g. the customize category tab icons the
    /// prefab leaves null. The painter loads each from the exported icon set and overrides the sprite.
    pub icons: Vec<(i64, String)>,
}

/// The exported RRUI icon for a customize category tab (the `[IconToggleChip] <Category>` chips), or None.
fn category_icon(category: &str) -> Option<&'static str> {
    Some(match category.trim() {
        "AvatarType" => "icon_AvatarCustomization_Body_Full.png",
        "Face" => "icon_AvatarCustomization_Face.png",
        "Hair" => "icon_AvatarCustomization_Hair.png",
        "Clothing" => "icon_AvatarCustomization_Torso.png",
        "Accessories" => "icon_AvatarCustomization_Accessories.png",
        "Feet" => "icon_AvatarCustomization_Shoes.png",
        "Custom" => "icon_AvatarCustomization_Custom.png",
        _ => return None,
    })
}

/// Where the exported RRUI icons live (override with METALLIC_WARDROBE_DIR).
fn icon_dir() -> PathBuf {
    std::env::var("METALLIC_WARDROBE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\Users\alexi\rr-capture\decomp-full\assets\AssetStudioExport"))
}

/// Rows the client adds to a page, copied from rows of the game's pages: a slider row, a switch row, and a radio
/// group (for choice dialogs).
#[derive(Default)]
pub struct Donors {
    pub slider: Option<Element>,
    pub switch: Option<Element>,
    pub radio: Option<Element>,
}

impl Donors {
    /// The Head Bob slider row of the Controls tab, the VSync switch row and the Graphics Quality radio group of the
    /// Visuals tab.
    pub fn find(controls: Option<&Element>, visuals: Option<&Element>) -> Donors {
        let row = |root: Option<&Element>, class: &str, name: &str| {
            root.and_then(|r| find(r, &|e| e.name.contains(name) && e.walk().iter().any(|(_, k)| k.impls().iter().any(|(c, _)| c.ends_with(class))))).cloned()
        };
        Donors {
            slider: row(controls, "/HeadBobSliderImpl", "[Slider"),
            switch: row(visuals, "/EnableVSyncToggleImpl", "[PanelWithPaddingHorizontal]"),
            radio: row(visuals, "/GraphicsQualityToggleImpl", "[RadioButtonSelection]"),
        }
    }
}

fn find<'a>(e: &'a Element, f: &dyn Fn(&Element) -> bool) -> Option<&'a Element> {
    if f(e) {
        return Some(e);
    }
    e.children.iter().find_map(|c| find(c, f))
}

fn find_mut<'a>(e: &'a mut Element, f: &dyn Fn(&Element) -> bool) -> Option<&'a mut Element> {
    if f(e) {
        return Some(e);
    }
    e.children.iter_mut().find_map(|c| find_mut(c, f))
}

fn has_component(e: &Element, id: i64) -> bool {
    e.components.iter().any(|c| c.id == id)
}

fn pptr(v: &Value, name: &str) -> Option<i64> {
    v.get(name).and_then(Value::as_pptr).filter(|p| p.file_id == 0 && !p.is_null()).map(|p| p.path_id)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn set(v: &mut Value, name: &str, new: Value) {
    if let Value::Struct { fields, .. } = v {
        match fields.iter_mut().find(|(n, _)| &**n == name) {
            Some((_, x)) => *x = new,
            None => fields.push((name.into(), new)),
        }
    }
}

/// Sets the text of the TMP text component `id`.
fn set_text(root: &mut Element, id: i64, text: &str) {
    if let Some(e) = find_mut(root, &|e| has_component(e, id)) {
        for c in e.components.iter_mut().filter(|c| c.id == id) {
            set(&mut c.value, "m_text", Value::String(text.to_string()));
        }
    }
}

/// Shows or hides the element carrying component `id` (a Hideable, usually).
fn show(root: &mut Element, id: i64, on: bool) {
    if let Some(e) = find_mut(root, &|e| has_component(e, id)) {
        e.active = on;
    }
}

/// `SetCamelCase` -> `set_camel_case`
fn snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// The setting a SettingsModelController switch or slider shows, by its impl.
fn setting(class: &str, d: &Value) -> Option<String> {
    let name = class.strip_prefix("SettingsModelController/")?;
    let n = |k: &str| d.get(k).and_then(Value::as_i64).unwrap_or(0);
    Some(match name {
        "InvertCameraToggleImpl" => (if n("Horizontal") == 1 { "invert_x" } else { "invert_y" }).to_string(),
        "EnableVSyncToggleImpl" => "vsync".to_string(),
        // axis 0 is the mouse, 1 and 2 the look axes (from the row labels)
        "CameraVerticalSensitivitySliderImpl" => ["mouse_sensitivity", "look_horizontal", "look_vertical"].get(n("axis") as usize)?.to_string(),
        // volume types from the Audio tab's row labels (0 overall, 1 in-room voice, 2 sounds, 3 music, 4 ambience,
        // 5 voice call, 6 other roomies, 7 game AI)
        "VolumeSliderImpl" => match n("Type") {
            0 => "volume_master".to_string(),
            1 => "volume_voice".to_string(),
            2 => "volume_sfx".to_string(),
            3 => "volume_music".to_string(),
            4 => "volume_ambience".to_string(),
            5 => "volume_voice_call".to_string(),
            6 => "volume_roomies".to_string(),
            7 => "volume_game_ai".to_string(),
            t => format!("volume_{t}"),
        },
        // radio groups pick one of several values; not handled yet
        _ if name.contains("Radio") || name.starts_with("GraphicsQuality") => return None,
        _ => snake(name.strip_suffix("ToggleImpl").or_else(|| name.strip_suffix("SliderImpl"))?),
    })
}

/// The setting a radio button sets and the value it sets it to: `XRadioToggleImpl` sets `x` to its one number field
/// (`Mode`, `Size`, ...); the graphics quality buttons set the quality level they map to on PC.
fn radio(class: &str, d: &Value) -> Option<(String, f64)> {
    let name = class.strip_prefix("SettingsModelController/")?;
    if name == "GraphicsQualityToggleImpl" {
        return Some(("graphics_quality".into(), d.get("defaultQualityMapping")?.as_i64()? as f64));
    }
    let base = name.strip_suffix("RadioToggleImpl")?;
    let Value::Struct { fields, .. } = d else { return None };
    let v = fields.iter().find_map(|(_, v)| matches!(v, Value::Int(_) | Value::UInt(_)).then(|| v.as_i64()).flatten())?;
    Some((snake(base), v as f64))
}

/// Dropdown buttons (`ShowXButtonImpl`, which open a radio-button dialog in the game): the setting, the dialog's title
/// and the options in enum order. Names INFERRED from the enum members (the game shows localized strings).
const DROPDOWNS: &[(&str, &str, &str, &[&str])] = &[
    ("ShowSprintModeSelectionButtonImpl", "sprint_input_mode", "Sprint Input Mode", &["Hold", "Toggle", "Press"]),
    ("ShowVoiceCallRoomVoiceChatModeDropdownButtonImpl", "voice_call_room_voice_mode", "While in Voice Call...", &["Room Voices On", "Mute Myself", "Room Voices Off"]),
];

/// The PC bit of GraphicsQualityToggleImpl.PlatformFlags (ANDROID, IOS, EDITOR, PC_AND_PCVR as 1, 2, 4, 8: INFERRED
/// order of the flag values).
const PC_AND_PCVR: i64 = 8;

/// Hide-if impls: Some(true) to hide their Hideable, Some(false) to show it, None to leave it. The client is on a PC
/// screen with mouse and keyboard (INFERRED: input type 1, from the `[MKB_ONLY]` rows), not a developer, not junior.
fn hidden(class: &str, d: &Value, ctx: &Ctx) -> Option<bool> {
    let name = class.rsplit('/').next().unwrap_or(class);
    let input = d.get("inputType").and_then(Value::as_i64);
    Some(match name {
        "HideIfBackStackEmptyImpl" => ctx.back_stack == 0,
        "HideIfBackStackNotEmptyImpl" => ctx.back_stack > 0,
        "HideIfScreensImpl" | "HideIfNotDeveloperImpl" | "HideIfExportNotAllowedImpl" | "HideIfControlSchemeCantBeChangedImpl"
        | "HideCachedLoginErrorImpl" | "HideLoginErrorImpl" | "HideIfHasNotRefreshedImpl" => true,
        "HideIfVRImpl" | "HideIfNotScreensImpl" | "HideIfJuniorImpl" | "HideIfWatchUIOnLockdownImpl" | "HideIfCanNotLogoutImpl"
        | "HideIfCanNotQuitImpl" | "HideIfUserIsOnPlatformImpl" | "HideIfGraphicsLockedImpl" => false,
        // no gamepad shoulder tabbing, nothing unread, an empty shopping bag; chat and creating rooms allowed
        "HideIfShoulderTabbingDisabled" | "HideIfNotJuniorImpl" | "HideIfNoUnreadNotificationsImpl"
        | "HideIfNoUnreadMessagesImpl" | "HideIfNoNavBarPeopleButtonUnreadImpl" | "HideIfShoppingBagEmptyImpl"
        | "HideIfShouldHideFriendsButtonBadgeImpl" => true,
        "HideIfCannotWatchChatImpl" | "HideIfCannotCreateRoomsImpl" => false,
        // placeholder data (the server has no friends, notifications, items or tokens for the menu yet): empty lists
        // show their empty-list messages, the no-search-results one stays hidden; the account a page shows is the
        // local player's (online, not a junior, not their own friend, not blocked or favourited)
        "HideIfNoUnreadFriendRequestsImpl" | "HideIfSearchResultExistsImpl" | "HideIfLocalAccountImpl" | "HideIfNotFriendImpl"
        | "HideIfNotBlockingLocalPlayerImpl" | "HideIfNotFavoritedByLocalPlayerImpl" | "HideIfNoTokenRestrictedBalanceImpl" => true,
        "HideIfItemsExistImpl" | "HideIfNotEmptyImpl" | "HideIfNoSubtabsImpl" | "HideIfNotLocalAccountImpl" | "HideIfJuniorAccountImpl"
        | "HideIfOfflineImpl" | "HideIfActuallyOfflineImpl" | "HideIfBlockingLocalPlayerImpl" | "HideIfBlockedByLocalPlayerImpl" => false,
        "HideIfEmptyImpl" => true,
        // title screen, signed in with no friends online (placeholder): collapse the "Join your friends" grid so the
        // panel is just the handle + Play, as the real client shows it when nobody's on. Not a high-friend-count user.
        "HideIfNoFriendsOnlineImpl" | "HideIfPlayerDoesntHaveHighFriendCountImpl" => true,
        "HideIfPlayerHasHighFriendCountImpl" => false,
        "HideIfScreensInputModeImpl" => input == Some(1),
        "HideIfNotScreensInputModeImpl" => input != Some(1),
        _ => return None,
    })
}

/// What an account or commerce text impl shows for the local player: the name (no separate display name yet),
/// level 1 and no tokens until the server has them (placeholders); the impl's `prefix` goes first.
fn account_text(class: &str, d: &Value, account: &str) -> Option<String> {
    let value = match class {
        "AccountModelController/DisplayNameTextImpl" | "AccountModelController/UsernameTextImpl" => account,
        // the title screen's signed-in handle: "@" + the account name (the prefab default is "@Username")
        "TitleScreenFlowModelController/CachedAccountUsernameTextImpl" => return Some(format!("@{account}")),
        "AccountModelController/LevelTextImpl" | "PlayerProgressionWatchHomeModelController/LevelTextImpl" => "1",
        "PlayerCommerceModelController/TokenBalanceTextImpl" => "0",
        _ => return None,
    };
    Some(format!("{}{value}", d.get("prefix").and_then(Value::as_str).unwrap_or("")))
}

/// Gives a copied subtree ids of its own (object ids xor `salt`), references inside it included.
fn fresh(e: &Element, salt: i64) -> Element {
    let mut ids = HashSet::new();
    for (_, k) in e.walk() {
        ids.extend([k.id, k.transform]);
        ids.extend(k.components.iter().map(|c| c.id));
    }
    fn fix(v: &mut Value, ids: &HashSet<i64>, salt: i64) {
        match v {
            Value::PPtr(p) if p.file_id == 0 && ids.contains(&p.path_id) => *p = PPtr { file_id: 0, path_id: p.path_id ^ salt },
            Value::Array(a) => a.iter_mut().for_each(|x| fix(x, ids, salt)),
            Value::Map(m) => m.iter_mut().for_each(|(a, b)| (fix(a, ids, salt), fix(b, ids, salt)).1),
            Value::Struct { fields, .. } => fields.iter_mut().for_each(|(_, x)| fix(x, ids, salt)),
            _ => {}
        }
    }
    let mut out = e.clone();
    out.each_mut(&mut |k| {
        (k.id, k.transform) = (k.id ^ salt, k.transform ^ salt);
        for c in &mut k.components {
            c.id ^= salt;
            fix(&mut c.value, &ids, salt);
        }
    });
    out
}

/// What a page's impls wire up: actions by selectable id, switches by ToggleTheme id, sliders by selectable id.
#[derive(Default)]
struct Wiring {
    actions: HashMap<i64, Action>,
    switches: Vec<(i64, String)>,
    sliders: Vec<(i64, String)>,
}

/// Adds a copy of `donor` after the element `after` names, wired to setting `key`, with `texts`: (name of the element
/// holding a text, its new text).
fn add_row(page: &mut Element, donor: &Element, after: &dyn Fn(&Element) -> bool, texts: &[(&str, &str)], key: &str, salt: i64, w: &mut Wiring) {
    let mut row = fresh(donor, salt);
    let mut wired = false;
    row.each_mut(&mut |e| {
        for (class, d) in e.impls() {
            if let (Some(_), Some(theme)) = (setting(class, d), pptr(d, "toggleTheme")) {
                w.switches.push((theme, key.to_string()));
                w.actions.insert(pptr(d, "button").unwrap_or(0), Action::Toggle(key.to_string()));
                wired = true;
            } else if let (Some(_), Some(s)) = (setting(class, d), pptr(d, "slider")) {
                w.sliders.push((s, key.to_string()));
                w.actions.insert(s, Action::Slider(key.to_string()));
                wired = true;
            }
        }
        // the copy's own controllers would wire it to the donor's setting
        e.components.retain(|c| c.value.get("implementation").is_none());
    });
    if !wired {
        return;
    }
    for (name, text) in texts {
        let holder = find(&row, &|e| e.name.starts_with(name));
        if let Some(id) = holder.and_then(|h| h.walk().iter().find_map(|(_, k)| k.component("TMPro.TextMeshProUGUI").map(|c| c.id))) {
            set_text(&mut row, id, text);
        }
    }
    insert(page, after, &mut Some(row));
}

/// Puts `row` right after the first element `after` names.
fn insert(e: &mut Element, after: &dyn Fn(&Element) -> bool, row: &mut Option<Element>) {
    if let Some(i) = e.children.iter().position(after) {
        if let Some(r) = row.take() {
            e.children.insert(i + 1, r);
        }
        return;
    }
    for c in &mut e.children {
        insert(c, after, row);
    }
}

/// The escape menu's added rows: copies of its Settings row without its controllers, each opening its page.
fn escape_rows(page: &mut Element, w: &mut Wiring) {
    let settings = |e: &Element| e.name == "[TextButton] Settings";
    let Some(donor) = find(page, &settings).cloned() else { return };
    // added in reverse, each right under the Settings row
    for (i, (label, path)) in ESCAPE_ROWS.iter().enumerate().rev() {
        let mut row = fresh(&donor, MADE | (0x10 + i as i64) << 40);
        row.name = format!("[TextButton] {label}");
        row.each_mut(&mut |e| e.components.retain(|c| c.value.get("implementation").is_none()));
        let first = |row: &Element, class: &str| row.walk().iter().find_map(|(_, k)| k.component(class).map(|c| c.id));
        if let Some(t) = first(&row, "TMPro.TextMeshProUGUI") {
            set_text(&mut row, t, label);
        }
        if let Some(b) = first(&row, "RRUI.MultiGraphicButton") {
            w.actions.insert(b, Action::Go { page: path.to_string(), tab: false });
        }
        insert(page, &settings, &mut Some(row));
    }
}

/// Unity's Slider: the fill reaches the value and the handle sits on it, both the full height of their boxes (left to
/// right sliders).
fn show_slider(root: &mut Element, id: i64, value: f32, range: Option<(f32, f32)>, shown: &str) {
    let Some(e) = find_mut(root, &|e| has_component(e, id)) else { return };
    let Some(c) = e.components.iter_mut().find(|c| c.id == id) else { return };
    if let Some((min, max)) = range {
        set(&mut c.value, "m_MinValue", Value::Float(min as f64));
        set(&mut c.value, "m_MaxValue", Value::Float(max as f64));
    }
    let v = &c.value;
    let num = |n: &str, d: f32| v.get(n).and_then(Value::as_f32).unwrap_or(d);
    let (min, max) = (num("m_MinValue", 0.0), num("m_MaxValue", 1.0));
    let t = if max > min { ((value - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
    let (fill, handle) = (pptr(v, "m_FillRect"), pptr(v, "m_HandleRect"));
    let labels = [("currentValueLabel", shown.to_string()), ("minValueLabel", min.round().to_string()), ("maxValueLabel", max.round().to_string())]
        .map(|(n, x)| (pptr(v, n), x));
    set(&mut c.value, "m_Value", Value::Float(value as f64));
    root.each_mut(&mut |e| {
        if Some(e.transform) == fill {
            (e.rect.anchor_min, e.rect.anchor_max) = ([0.0, 0.0], [t, 1.0]);
        }
        if Some(e.transform) == handle {
            (e.rect.anchor_min, e.rect.anchor_max) = ([t, 0.0], [t, 1.0]);
        }
    });
    for (label, x) in labels {
        if let Some(l) = label {
            set_text(root, l, &x);
        }
    }
}

/// Our ranges for the settings the game has no slider for.
fn range(key: &str) -> Option<(f32, f32)> {
    match key {
        "fov" => Some((40.0, 110.0)),
        "resolution" => Some((0.0, (RESOLUTIONS.len() - 1) as f32)),
        _ => None,
    }
}

/// What a slider shows as its value: the window height for the window size, else the number.
fn shown(key: &str, v: f64) -> String {
    match key {
        "resolution" => format!("{}p", RESOLUTIONS[(v.max(0.0) as usize).min(RESOLUTIONS.len() - 1)].1),
        _ => format!("{}", v.round()),
    }
}

/// `template` as the page shows now: what its impls do under `ctx`, themed.
pub fn build(template: &Element, ctx: &Ctx, theme: &Theme, donors: &Donors) -> Built {
    let mut page = template.clone();
    let mut w = Wiring::default();
    let leaf = routes::leaf(ctx.page);
    if leaf == ESCAPE {
        escape_rows(&mut page, &mut w);
    }
    // our rows: under the VSync switch on the Visuals tab
    if leaf == VISUALS {
        let vsync = |e: &Element| e.name.starts_with("[PanelWithPaddingHorizontal]") && e.children.iter().any(|k| k.name.starts_with("VSync"));
        // added in reverse, each right under the VSync row
        if let Some(d) = &donors.slider {
            let (min, max) = ("[Text] MinLabel", "[Text] Max Label");
            add_row(&mut page, d, &vsync, &[("[Text] Primary", "Field of View"), (min, "40"), (max, "110")], "fov", 3 << 56, &mut w);
            add_row(&mut page, d, &vsync, &[("[Text] Primary", "Window Size"), (min, "720p"), (max, "2160p")], "resolution", 2 << 56, &mut w);
        }
        if let Some(d) = &donors.switch {
            add_row(&mut page, d, &vsync, &[("[Text] Primary", "Fullscreen"), ("[Text] Secondary", "Fills the screen, without a window border.")], "fullscreen", 1 << 56, &mut w);
        }
    }
    let mut hides: Vec<(i64, bool)> = vec![];
    let mut texts: Vec<(i64, String)> = vec![];
    let mut active = HashSet::new();
    let mut cards = None;
    let mut room_list = None;
    // extra sprite overrides the client composes (e.g. the title footer's Authorize Device shield), merged into `icons`
    let mut footer_icons: Vec<(i64, String)> = vec![];
    for (_, e) in page.walk() {
        for (class, d) in e.impls() {
            let name = class.rsplit('/').next().unwrap_or(class);
            if let (Some(h), Some(hide)) = (pptr(d, "hideable"), hidden(class, d, ctx)) {
                hides.push((h, hide));
            }
            let button = pptr(d, "button");
            let action = match name {
                "NavigateBackButtonImpl" => Some(Action::Back),
                "CloseWatchMenuButtonImpl" => Some(Action::Close),
                "LogoutButtonImpl" | "GoToSwitchAccountPageButtonImpl" => Some(Action::SwitchAccount),
                "QuitButtonImpl" => Some(Action::Quit),
                "GoToLoginPageButtonImpl" => Some(Action::Go { page: LOGIN.into(), tab: false }),
                "ManualLoginButtonImpl" => Some(Action::Login),
                // links (LinkButtonImpl, LinkUriButtonImpl) and the watch's tabs (WatchTabsImpl, PlayTabImpl, ...)
                _ if name.starts_with("Link") || name.ends_with("TabImpl") || name.ends_with("TabsImpl") => {
                    let to: Option<Vec<&str>> = routes::uri(d).iter().map(|r| page_of(r)).collect();
                    to.filter(|t| !t.is_empty()).map(|t| {
                        let watch_tab = !name.starts_with("Link");
                        let group = d.get("treatAsPageGroup").and_then(Value::as_bool).unwrap_or(false);
                        let path = routes::target(ctx.page, &t, watch_tab);
                        if routes::current(ctx.page, &path, t.len(), watch_tab || group) {
                            active.extend(pptr(d, "toggleTheme"));
                        }
                        let in_watch = ctx.page.starts_with(routes::ROOT);
                        Action::Go { page: path, tab: group || in_watch && watch_tab || leaf != ESCAPE && name == "LinkButtonImpl" }
                    })
                }
                // the store button names no route: the store host (INFERRED from the root screen's prefill routes)
                "GoToStoreScreenButtonController" => {
                    Some(Action::Go { page: routes::target(ctx.page, &["(Navigation_Store)"], true), tab: ctx.page.starts_with(routes::ROOT) })
                }
                _ => None,
            };
            if let (Some(b), Some(a)) = (button, action) {
                w.actions.insert(b, a);
            }
            if name == "LogoutButtonTextImpl" {
                texts.extend(pptr(d, "textMeshProUGUI").map(|t| (t, "Switch Account".to_string())));
            }
            if let Some(text) = account_text(class, d, ctx.account) {
                texts.extend(pptr(d, "textMeshProUGUI").map(|t| (t, text)));
            }
            if name == "ManualLoginUsernameInputFieldImpl" {
                if let Some(f) = pptr(d, "inputField") {
                    w.actions.insert(f, Action::Field);
                    let field = e.components.iter().find(|c| c.id == f);
                    let text = field.and_then(|c| pptr(&c.value, "m_TextComponent"));
                    let hint = field.and_then(|c| pptr(&c.value, "m_Placeholder"));
                    let caret = if ctx.focused { "|" } else { "" };
                    texts.extend(text.map(|t| (t, format!("{}{caret}", ctx.typed))));
                    if let Some(h) = hint {
                        hides.push((h, !ctx.typed.is_empty() || ctx.focused));
                    }
                }
            }
            if name == "CachedAccountCardSpawnerImpl" {
                cards = pptr(d, "recyclingListScrollRect");
            }
            if name == "VirtualizedRoomListCardSpawnerImpl" {
                room_list = pptr(d, "virtualizedList");
            }
            if let (Some((key, value)), Some(b), Some(t)) = (radio(class, d), button, pptr(d, "toggleTheme")) {
                w.actions.insert(b, Action::Choose(key.clone(), value));
                if ctx.settings.get(&key) == value {
                    active.insert(t);
                }
            }
            if name == "GraphicsQualityToggleImpl" && d.get("supportedPlatforms").and_then(Value::as_i64).is_some_and(|p| p & PC_AND_PCVR == 0) {
                hides.extend(pptr(d, "radioOptionHideable").map(|h| (h, true)));
            }
            if let (Some((_, key, _, options)), Some(b)) = (DROPDOWNS.iter().find(|x| x.0 == name), button) {
                w.actions.insert(b, Action::Dropdown(key.to_string()));
                let i = (ctx.settings.get(key).max(0.0) as usize).min(options.len() - 1);
                texts.extend(pptr(d, "text").map(|t| (t, options[i].to_string())));
            }
            if let Some(key) = setting(class, d) {
                if let (Some(b), Some(t)) = (button, pptr(d, "toggleTheme")) {
                    w.actions.insert(b, Action::Toggle(key.clone()));
                    w.switches.push((t, key));
                } else if let Some(s) = pptr(d, "slider") {
                    w.actions.insert(s, Action::Slider(key.clone()));
                    w.sliders.push((s, key));
                }
            }
        }
    }
    // a Hideable several controllers drive is hidden when any of them hides it (INFERRED from RRUI's Hideable)
    let mut hidden_by: HashMap<i64, bool> = HashMap::new();
    for (h, hide) in hides {
        *hidden_by.entry(h).or_default() |= hide;
    }
    for (h, hide) in hidden_by {
        show(&mut page, h, !hide);
    }
    // the login page logs in by name only: passwords, password resets, new accounts and the terms are not ours
    if leaf == LOGIN {
        const NOT_OURS: [&str; 5] = ["[TextFieldSingleLine] Password", "[Checkbox]", "[TextButton] Forgot password", "[TextButton] Create an account", "(TermsOfServiceButton)"];
        page.each_mut(&mut |e| e.active &= !NOT_OURS.iter().any(|n| e.name.starts_with(n)));
    }
    // the signed-in title screen with no friend data: collapse the "Join your friends" grid (its cards are the
    // prefab's placeholder template, filled by the account list controller at runtime) and the boot-time Terms/Update
    // prompts, so the panel is the handle + Play over the avatar — the real front door when nobody's online.
    if std::env::var("METALLIC_UI_DUMP").is_ok() && ctx.page.contains("TitleScreen") {
        eprintln!("[ui] === build page={} leaf={} ===", ctx.page, leaf);
        for (d, e) in page.walk() {
            let img = e.component("UnityEngine.UI.Image").map(|c| pptr(&c.value, "m_Sprite").map(|_| "sprite").unwrap_or("Image"));
            let txt = e.component("TMPro.TextMeshProUGUI").and_then(|c| c.value.get("m_text").and_then(Value::as_str)).map(|s| s.chars().take(24).collect::<String>());
            let grad = e.has("RRUI.MultiGraphicButton") || e.has("UnityEngine.UI.Button");
            eprintln!("[ui] {:width$}{} active={}{}{}{}", "", e.name, e.active,
                img.map(|s| format!(" [{s}]")).unwrap_or_default(),
                if grad { " [BTN]" } else { "" },
                txt.map(|t| format!(" text={t:?}")).unwrap_or_default(), width = d * 2);
        }
    }
    if leaf == TITLE_CACHED {
        // Compose the real title video's ONE white card: @handle, PLAY, SWITCH ACCOUNT, and the Language/Authorize
        // footer, all in a single panel. Hide the friends grid + boot prompts + the "…" Player Menu button + the
        // language blurb + the divider + the now-unused separate Call-to-Action panel.
        const HIDE: [&str; 7] = ["OnlineFriends", "(TermsOfServiceButton)", "[TextButton] Update", "[IconButton] Player Menu",
            "[ParagraphText] Language Warning", "[HorizontalDivider]", "[PanelWithPaddingVertical] Call to Action"];
        page.each_mut(&mut |e| e.active &= !HIDE.iter().any(|n| e.name.starts_with(n)));
        // The "Dropdown menu" element is a popup overlay holding the real Switch Account button + Language icon-button.
        // Pull those two out (moving, not cloning, keeps component ids unique) and add an Authorize Device button beside
        // Language — a `fresh` (re-id'd) copy of the Language icon-text button with the real SafeModeShield icon. Visual
        // only: the OAuth device-code flow is wired to our own backend at the account step (rec.net's auth is gone).
        const AUTH_SALT: i64 = 0x5a1d << 40;
        let mut moved = vec![];
        if let Some(dd) = find_mut(&mut page, &|e| e.name == "Dropdown menu") {
            let mut keep = vec![];
            for mut c in dd.children.drain(..) {
                if c.name == "[PanelHorizontal]" {
                    // the footer row: add Authorize Device beside Language
                    if let Some(lang) = c.children.iter().find(|k| k.name.starts_with("[IconTextButton_OneLine] Language")).cloned() {
                        let mut auth = fresh(&lang, AUTH_SALT);
                        auth.name = "[IconTextButton_OneLine] Authorize Device".into();
                        auth.active = true;
                        if let Some(tid) = auth.walk().iter().find_map(|(_, k)| k.component("TMPro.TextMeshProUGUI").map(|c| c.id)) {
                            set_text(&mut auth, tid, "Authorize Device");
                        }
                        if let Some(iid) = find(&auth, &|e| e.name.starts_with("Icon - Change Icon")).and_then(|e| e.component("UnityEngine.UI.Image").map(|c| c.id)) {
                            footer_icons.push((iid, "MenuIcon_Sprites_3(HR)_SafeModeShield.png".into()));
                        }
                        c.children.push(auth);
                    }
                    c.active = true;
                    moved.push(c);
                } else if c.name.starts_with("[TextButton] Switch account") {
                    c.active = true;
                    moved.push(c);
                } else {
                    keep.push(c);
                }
            }
            dd.children = keep;
            dd.active = false;
        }
        // The Player Section panel already contains an (inactive) Play button alongside the @handle header — activate it
        // and stack Switch Account + the footer row after it, so one card holds @handle, PLAY, SWITCH ACCOUNT, footer.
        if let Some(ps) = find_mut(&mut page, &|e| e.name.starts_with("[PanelWithPaddingVertical] Player Section")) {
            if let Some(play) = ps.children.iter_mut().find(|c| c.name.starts_with("[TextButton] Play")) {
                play.active = true;
            }
            ps.children.extend(moved);
        }
        // uppercase the two primary button labels (PLAY / SWITCH ACCOUNT), as the real buttons render them
        for (name, label) in [("[TextButton] Play", "PLAY"), ("[TextButton] Switch account", "SWITCH ACCOUNT")] {
            if let Some(id) = find(&page, &|e| e.active && e.name.starts_with(name))
                .and_then(|p| p.walk().iter().find_map(|(_, k)| k.component("TMPro.TextMeshProUGUI").map(|c| c.id)))
            {
                texts.push((id, label.to_string()));
            }
        }
        // the Switch Account button goes to the switch-account page (the real LogoutButton/GoToSwitchAccount action)
        if let Some(id) = find(&page, &|e| e.name.starts_with("[TextButton] Switch account"))
            .and_then(|p| p.walk().iter().find_map(|(_, k)| k.component("RRUI.MultiGraphicButton").map(|c| c.id)))
        {
            w.actions.insert(id, Action::SwitchAccount);
        }
        // the big Play button dismisses the title into the game (the room already loads behind it)
        if let Some(id) = find(&page, &|e| e.active && e.name.starts_with("[TextButton] Play"))
            .and_then(|p| p.walk().iter().find_map(|(_, k)| k.component("RRUI.MultiGraphicButton").map(|c| c.id)))
        {
            w.actions.insert(id, Action::Close);
        }
    }
    // the watch shows no tutorial highlight (the root screen holds a sample one, shown only while a tutorial runs)
    if ctx.page.starts_with(routes::ROOT) {
        page.each_mut(&mut |e| e.active &= e.name != "HighlightFadeout");
    }
    for (t, text) in texts {
        set_text(&mut page, t, &text);
    }
    for (theme_id, key) in &w.switches {
        if ctx.settings.on(key) {
            active.insert(*theme_id);
        }
    }
    for (s, key) in &w.sliders {
        show_slider(&mut page, *s, ctx.settings.get(key) as f32, range(key), &shown(key, ctx.settings.get(key)));
    }
    if let Some(list) = cards {
        account_cards(&mut page, list, ctx, &mut w.actions);
    }
    if let Some(list) = room_list {
        room_cards(&mut page, list, ctx.rooms, &mut w.actions);
    }
    let dialog = ctx.choosing.and_then(|k| DROPDOWNS.iter().find(|d| d.1 == k));
    if let (Some((_, key, title, options)), Some(donor)) = (dialog, &donors.radio) {
        choice_dialog(&mut page, donor, key, title, options, ctx.settings.get(key), &mut w.actions, &mut active);
    }
    // scroll views: the content moves up by the offset
    let mut scrolls = vec![];
    for (_, e) in page.walk() {
        if let Some(c) = e.component("UnityEngine.UI.ScrollRect") {
            if let (Some(content), Some(viewport)) = (pptr(&c.value, "m_Content"), pptr(&c.value, "m_Viewport").or(Some(e.transform))) {
                scrolls.push((c.id, content, viewport));
            }
        }
    }
    for (id, content, _) in &scrolls {
        let y = ctx.scroll.get(id).copied().unwrap_or(0.0);
        if let Some(e) = find_mut(&mut page, &|e| e.transform == *content) {
            e.rect.anchored_position[1] += y;
        }
    }
    // the customize category tabs: each `[IconToggleChip] <Category>` has an Image whose sprite the prefab leaves null
    // (the game assigns it at runtime). Record its id + the real icon so the painter overrides it.
    let mut icons: Vec<(i64, String)> = vec![];
    for (_, e) in page.walk() {
        let Some(cat) = e.name.strip_prefix("[IconToggleChip] ") else { continue };
        let Some(file) = category_icon(cat) else { continue };
        let id = e.walk().iter().find_map(|(_, k)| {
            if k.name.starts_with("Image - Change Icon") {
                k.component("UnityEngine.UI.Image").map(|c| c.id)
            } else {
                None
            }
        });
        if let Some(id) = id {
            icons.push((id, file.to_string()));
        }
    }
    icons.extend(footer_icons);
    // fill the Customize item grids with real wardrobe items, and the colour grids with the real swatches
    if let Some(card) = ctx.item_card {
        item_cards(&mut page, card, &mut w.actions);
    }
    color_swatches(&mut page, &mut w.actions);
    theme.apply_with(&mut page, &active);
    Built { page, actions: w.actions, scrolls, icons }
}

fn strukt(fields: Vec<(&str, Value)>) -> Value {
    Value::Struct { type_name: "".into(), fields: fields.into_iter().map(|(k, v)| (k.into(), v)).collect() }
}

/// A component made here rather than read from a prefab.
fn made(class: &str, id: i64, fields: Vec<(&str, Value)>) -> prefab::Component {
    prefab::Component { class: class.into(), value: strukt(fields), file: None, id, enabled: true, themed: Default::default() }
}

/// A LayoutElement that takes its element out of its parent's layout group (page roots often have one).
fn free(id: i64) -> prefab::Component {
    made("UnityEngine.UI.LayoutElement", id, vec![("m_IgnoreLayout", Value::Bool(true))])
}

/// Ids of the elements and components the client makes (high bits no prefab object id was seen to use: INFERRED).
const MADE: i64 = 0x7e5a << 48;

/// The choice dialog of a dropdown: the screen dimmed (a click on it closes the dialog) and, in the middle, a copy of
/// the game's radio group (the Graphics Quality one) titled `title` with one radio button per option, the current
/// one on. INFERRED: the game shows these choices as a radio-button dialog (DynamicRadioButtonData); its dialog
/// prefab is not read, the radio group stands in for it.
#[allow(clippy::too_many_arguments)]
fn choice_dialog(page: &mut Element, donor: &Element, key: &str, title: &str, options: &[&str], current: f64, actions: &mut HashMap<i64, Action>, active: &mut HashSet<i64>) {
    let color = |r: f64, g: f64, b: f64, a: f64| strukt(vec![("r", Value::Float(r)), ("g", Value::Float(g)), ("b", Value::Float(b)), ("a", Value::Float(a))]);
    let dim = MADE | 1;
    let full = prefab::RectRules { anchor_min: [0.0; 2], anchor_max: [1.0; 2], size_delta: [0.0; 2], ..Default::default() };
    let dimmer = Element {
        name: "(dialog dimmer)".into(), active: true, id: MADE, transform: MADE | 2, rect: full, children: vec![],
        components: vec![
            made("UnityEngine.UI.Image", MADE | 3, vec![("m_Color", color(0.0, 0.0, 0.0, 0.55)), ("m_RaycastTarget", Value::Bool(true))]),
            made("UnityEngine.UI.Button", dim, vec![("m_Interactable", Value::Bool(true)), ("m_Colors", strukt(vec![])), ("m_Transition", Value::Int(0))]),
            free(MADE | 4),
        ],
    };
    actions.insert(dim, Action::CloseDialog);
    let mut panel = fresh(donor, MADE | 1 << 40);
    // the copy's controllers would wire it to graphics quality
    panel.each_mut(&mut |e| e.components.retain(|c| c.value.get("implementation").is_none()));
    panel.rect = prefab::RectRules { anchor_min: [0.5; 2], anchor_max: [0.5; 2], size_delta: [1323.0, 241.0], ..Default::default() };
    panel.components.insert(0, free(MADE | 5));
    let header = find(&panel, &|e| e.name.starts_with("[InputHeader]")).and_then(|h| h.walk().iter().find_map(|(_, k)| k.component("TMPro.TextMeshProUGUI").map(|c| c.id)));
    if let Some(id) = header {
        set_text(&mut panel, id, title);
    }
    let is_row = |e: &Element| e.children.iter().any(|k| k.name.starts_with("[RadioButton]"));
    let Some(row) = find_mut(&mut panel, &is_row) else { return };
    // the Low button: shown on PC (Fast is not)
    let Some(template) = row.children.iter().find(|k| k.name.starts_with("[RadioButton] (1)")).or(row.children.first()).cloned() else { return };
    row.children.clear();
    for (i, label) in options.iter().enumerate() {
        let mut option = fresh(&template, MADE | (i as i64 + 2) << 40);
        option.active = true;
        let text = option.walk().iter().find_map(|(_, k)| k.component("TMPro.TextMeshProUGUI").map(|c| c.id));
        if let Some(t) = text {
            set_text(&mut option, t, label);
        }
        for (_, e) in option.walk() {
            if let Some(b) = e.component("RRUI.MultiGraphicButton") {
                actions.insert(b.id, Action::Choose(key.to_string(), i as f64));
            }
            if let Some(t) = e.component("RRUI.Theme.ToggleTheme").filter(|_| i as f64 == current) {
                active.insert(t.id);
            }
        }
        row.children.push(option);
    }
    page.children.push(dimmer);
    page.children.push(panel);
}

/// The canvas a page is laid out on. INFERRED: the escape menu's host canvas is not in its prefab (this size fits
/// its content and our rows); the title pages use the title surface, 1920 x 1080, the settings pages alone a
/// watch-shaped one, the watch its root's surface (2032 x 1230, which holds the 1920 x 1080 router and the navigation
/// bar) and other pages the 1920 x 1080 their roots are made for.
pub fn canvas(page: &str) -> [f32; 2] {
    if page.starts_with(routes::ROOT) {
        return [2032.0, 1230.0];
    }
    match page {
        ESCAPE => [1000.0, 1150.0 + 140.0 * ESCAPE_ROWS.len() as f32],
        _ if page.starts_with("(Watch_Settings") => [1600.0, 1000.0],
        _ => [1920.0, 1080.0],
    }
}

/// A page to paint, with the state it shows.
#[derive(Clone, Debug, Default)]
pub struct Request {
    pub page: String,
    pub back_stack: usize,
    pub settings: Settings,
    pub account: String,
    pub typed: String,
    pub focused: bool,
    pub scroll: HashMap<i64, f32>,
    pub choosing: Option<String>,
    /// the name server, where room lists come from
    pub server: String,
    /// the login's access token (empty until signed in): room lists and player counts are fetched with it
    pub token: String,
}

/// A painted page: what to draw and what its selectables do.
pub struct Painted {
    pub page: String,
    pub drawn: Drawn,
    pub actions: HashMap<i64, Action>,
    pub scrolls: Vec<(i64, i64, i64)>,
}

/// Paints pages on a thread of its own, which keeps the game's files open and the sprites and fonts it has read.
pub struct Painter {
    ask: Sender<Request>,
    done: Mutex<Receiver<Result<Painted, String>>>,
}

impl Painter {
    pub fn start(bundle_dir: PathBuf) -> Painter {
        let (ask, asked) = channel::<Request>();
        let (tell, done) = channel();
        std::thread::spawn(move || {
            let mut lib = match Library::new(&bundle_dir) {
                Ok(l) => l,
                Err(e) => return drop(tell.send(Err(format!("{}: {e}", bundle_dir.display())))),
            };
            let pages = Pages::open(&bundle_dir);
            let theme = pages.as_ref().map_err(Clone::clone).and_then(|p| Theme::load(&mut lib, p, &bundle_dir));
            let (pages, theme) = match (pages, theme) {
                (Ok(p), Ok(t)) => (p, t),
                (Err(e), _) | (_, Err(e)) => return drop(tell.send(Err(e))),
            };
            let mut looks = Looks::new(&mut lib);
            // by path: the page and its path with the prefill pages added
            let mut templates: HashMap<String, (Element, String)> = HashMap::new();
            // a path's pages, each in the Router of the one before; a router left empty shows its first prefill route
            let mut template = |looks: &mut Looks, page: &str| -> Result<(Element, String), String> {
                if !templates.contains_key(page) {
                    let mut path = page.to_string();
                    let mut parts = vec![];
                    for a in routes::parts(page) {
                        let mut e = prefab::load(looks.library(), &pages, &bundle_dir, a)?;
                        if a == routes::ROOT {
                            prefab::spawn_referenced(looks.library(), &pages, &bundle_dir, &mut e);
                        }
                        parts.push(e);
                    }
                    while let Some(next) = parts.last().and_then(prefill).and_then(|r| page_of(&r)).filter(|_| parts.len() < 6) {
                        parts.push(prefab::load(looks.library(), &pages, &bundle_dir, next)?);
                        path = format!("{path}{}{next}", routes::SEP);
                    }
                    let built = if parts.len() == 1 { parts.pop() } else { routes::nest(parts) };
                    templates.insert(page.to_string(), (built.ok_or_else(|| format!("{page}: a page without a Router holds another"))?, path));
                }
                Ok(templates[page].clone())
            };
            let mut donors = None;
            // the list, when and with which token it was fetched
            let mut room_list: Option<(std::time::Instant, String, Vec<RoomInfo>)> = None;
            // decoded customize-category icons, kept so each PNG is read once
            let mut icon_cache: HashMap<String, Option<std::sync::Arc<paint::Sprite>>> = HashMap::new();
            // the avatar-item card template, loaded once, to fill the Customize grids
            let item_card = prefab::load(looks.library(), &pages, &bundle_dir, ITEM_CARD).ok();
            while let Ok(mut r) = asked.recv() {
                // only the newest request matters
                while let Ok(newer) = asked.try_recv() {
                    r = newer;
                }
                if donors.is_none() && routes::leaf(&r.page).starts_with("(Watch_Settings") {
                    let (c, v) = (template(&mut looks, CONTROLS).ok().map(|t| t.0), template(&mut looks, VISUALS).ok().map(|t| t.0));
                    donors = Some(Donors::find(c.as_ref(), v.as_ref()));
                }
                let painted = template(&mut looks, &r.page).map(|(t, path)| {
                    let lists_rooms = t.walk().iter().any(|(_, e)| e.impls().iter().any(|(c, _)| c.ends_with("/VirtualizedRoomListCardSpawnerImpl")));
                    if lists_rooms && !r.server.is_empty() && room_list.as_ref().is_none_or(|(at, token, _)| at.elapsed().as_secs() >= 10 || *token != r.token) {
                        room_list = Some((std::time::Instant::now(), r.token.clone(), rooms::fetch(&r.server, &r.token).unwrap_or_else(|e| {
                            eprintln!("rooms: {e}");
                            vec![]
                        })));
                    }
                    let none = Donors::default();
                    let ctx = Ctx { page: &path, back_stack: r.back_stack, settings: &r.settings, account: &r.account, typed: &r.typed, focused: r.focused,
                        scroll: &r.scroll, choosing: r.choosing.as_deref(), rooms: room_list.as_ref().map(|(_, _, l)| l.as_slice()).unwrap_or_default(), item_card: item_card.as_ref() };
                    let b = build(&t, &ctx, &theme, donors.as_ref().unwrap_or(&none));
                    // place the real customize-category icons (the prefab leaves their sprite null)
                    looks.clear_overrides();
                    for (id, file) in &b.icons {
                        let sprite = icon_cache
                            .entry(file.clone())
                            .or_insert_with(|| paint::load_icon_sprite(&icon_dir().join(file), [0x33, 0x34, 0x3a]))
                            .clone();
                        if let Some(s) = sprite {
                            looks.override_sprite(*id, s);
                        }
                    }
                    Painted { drawn: paint::page_with(&mut looks, &b.page, canvas(&r.page)), page: r.page, actions: b.actions, scrolls: b.scrolls }
                });
                if tell.send(painted).is_err() {
                    return;
                }
            }
        });
        Painter { ask, done: Mutex::new(done) }
    }

    pub fn ask(&self, r: Request) {
        let _ = self.ask.send(r);
    }

    /// A page painted since the last call, if any.
    pub fn take(&self) -> Option<Result<Painted, String>> {
        self.done.lock().unwrap().try_recv().ok()
    }
}

/// The account switcher's list: one card per account this PC has used, made from the list's item template, the
/// current account first, in a row (the recycling list places its items itself; INFERRED spacing).
fn account_cards(page: &mut Element, list: i64, ctx: &Ctx, actions: &mut HashMap<i64, Action>) {
    let Some(holder) = find_mut(page, &|e| has_component(e, list)) else { return };
    let Some(template) = holder.children.iter().find(|c| c.name.ends_with(TEMPLATE)).cloned() else { return };
    let content = holder.components.iter().find(|c| c.id == list).and_then(|c| c.value.path("_Params/_Content")).and_then(Value::as_pptr).map(|p| p.path_id);
    let mut names: Vec<&str> = vec![ctx.account];
    names.extend(ctx.settings.accounts.iter().map(String::as_str).filter(|a| *a != ctx.account && !a.is_empty()));
    let Some(content) = find_mut(holder, &|e| Some(e.transform) == content) else { return };
    let size = template.rect.size_delta;
    for (i, name) in names.iter().enumerate() {
        let note = if i == 0 { "Playing now" } else { "" };
        let mut card = card(&template, (i as i64 + 1) << 52, name, note, "", Action::Account(name.to_string()), actions);
        let r = &mut card.rect;
        (r.anchor_min, r.anchor_max, r.pivot) = ([0.0, 0.5], [0.0, 0.5], [0.0, 0.5]);
        r.anchored_position = [120.0 + i as f32 * (size[0] + 40.0), 0.0];
        content.children.push(card);
    }
}

/// Fills each avatar-item grid (an `AvatarCustomizationItemListModel`, keyed by `outfitType`) with a card per real
/// wardrobe item for that slot, laid out in a grid — the way `room_cards` fills the rooms list, but from the external
/// `(AvatarCustomizationItemCard)` template.
fn item_cards(page: &mut Element, template: &Element, actions: &mut HashMap<i64, Action>) {
    use crate::avatar::real::{scan_slot, BodyType, Slot};
    let slot_for = |ot: i32| -> Option<Slot> {
        Some(match ot {
            2 => Slot::Hair,
            101 => Slot::Top,
            300 => Slot::Bottom,
            301 => Slot::Shoes,
            0 => Slot::Hat,
            20 => Slot::Beard,
            10 => Slot::Glasses,
            _ => return None,
        })
    };
    // each item list: its outfitType and the transform id of the element the cards spawn under
    let mut lists: Vec<(i32, i64)> = vec![];
    for (_, e) in page.walk() {
        let Some(model) = e.components.iter().find(|c| c.class == "RRUI.Data.AvatarCustomizationItemListModel") else { continue };
        let Some(ot) = model.value.get("outfitType").and_then(Value::as_i64) else { continue };
        let target = e.walk().iter().find_map(|(_, k)| {
            k.component("RRUI.Data.InfiniteListDataProvider").and_then(|p| pptr(&p.value, "prefabSpawnTarget"))
        });
        if let Some(t) = target {
            lists.push((ot as i32, t));
        }
    }
    let (cw, ch) = (template.rect.size_delta[0].max(210.0), template.rect.size_delta[1].max(280.0));
    let cols = 5usize;
    for (n, (ot, target)) in lists.into_iter().enumerate() {
        let Some(slot) = slot_for(ot) else { continue };
        let items = scan_slot(BodyType::Full, slot);
        let Some(content) = find_mut(page, &|e| e.transform == target) else { continue };
        for (i, name) in items.iter().enumerate() {
            let salt = MADE | (((n as i64) * 128 + 0x40 + i as i64) << 40);
            let mut c = card(template, salt, &prettify(name), "", "", Action::WearItem(ot, name.clone()), actions);
            let (col, row) = (i % cols, i / cols);
            let r = &mut c.rect;
            (r.anchor_min, r.anchor_max, r.pivot) = ([0.0, 1.0], [0.0, 1.0], [0.0, 1.0]);
            r.size_delta = [cw, ch];
            r.anchored_position = [32.0 + col as f32 * (cw + 22.5), -50.0 - row as f32 * (ch + 22.5)];
            content.children.push(c);
        }
    }
}

/// The real Rec Room skin tones (ColorsResourcesConfig.skinColors, sRGB), in order.
const SKIN_COLORS: [[u8; 3]; 20] = [
    [0x33, 0x1b, 0x13], [0x34, 0x24, 0x1c], [0x35, 0x29, 0x24], [0x44, 0x24, 0x1a], [0x54, 0x2f, 0x23], [0x5d, 0x42, 0x36], [0x7d, 0x48, 0x2e],
    [0x89, 0x5d, 0x43], [0xad, 0x5c, 0x38], [0xbd, 0x5f, 0x4c], [0xa6, 0x71, 0x53], [0xc1, 0x76, 0x57], [0xd4, 0x7b, 0x65], [0xb6, 0x8a, 0x66],
    [0xc7, 0x90, 0x80], [0xcb, 0x90, 0x6c], [0xbd, 0x94, 0x7b], [0xd2, 0xa5, 0x83], [0xd0, 0xa4, 0x90], [0xed, 0xd3, 0xc7],
];
/// The real Rec Room natural hair colours (ColorsResourcesConfig.naturalHairColors, sRGB), in order.
const HAIR_COLORS: [[u8; 3]; 17] = [
    [0x1b, 0x0f, 0x06], [0x23, 0x1e, 0x1a], [0x38, 0x1a, 0x03], [0x46, 0x13, 0x05], [0x57, 0x18, 0x09], [0x51, 0x2e, 0x15], [0x57, 0x35, 0x09],
    [0x80, 0x58, 0x23], [0x9f, 0x29, 0x0d], [0xa2, 0x41, 0x25], [0xba, 0x5e, 0x37], [0xbe, 0x70, 0x48], [0xc3, 0x86, 0x36], [0xd0, 0xb0, 0x86],
    [0x7c, 0x78, 0x77], [0xa4, 0xae, 0xbc], [0xb4, 0xae, 0xa9],
];

/// Fills each colour grid (an `AvatarColorListModel`, keyed by `avatarColorType`) with the real Rec Room swatches,
/// cloning the page's `[CardBackground] Color Swatch` template and setting each clone's Background Image colour.
fn color_swatches(page: &mut Element, actions: &mut HashMap<i64, Action>) {
    let palette = |ct: i32| -> Option<&'static [[u8; 3]]> {
        match ct {
            0 => Some(&SKIN_COLORS), // Skin
            1 | 2 => Some(&HAIR_COLORS), // HairPrimary / HairHighlight
            _ => None,
        }
    };
    // each colour list: its avatarColorType, the spawn-target transform id, and a clone of its swatch template
    let mut lists: Vec<(i32, i64, Element)> = vec![];
    for (_, e) in page.walk() {
        let Some(model) = e.components.iter().find(|c| c.class == "RRUI.Data.AvatarColorListModel") else { continue };
        let Some(ct) = model.value.get("avatarColorType").and_then(Value::as_i64) else { continue };
        let target = e.walk().iter().find_map(|(_, k)| {
            k.component("RRUI.Data.InfiniteListDataProvider").and_then(|p| pptr(&p.value, "prefabSpawnTarget"))
        });
        let template = e.walk().iter().find(|(_, k)| k.name.starts_with("[CardBackground] Color Swatch")).map(|(_, k)| (*k).clone());
        if let (Some(t), Some(tmpl)) = (target, template) {
            lists.push((ct as i32, t, tmpl));
        }
    }
    for (n, (ct, target, template)) in lists.into_iter().enumerate() {
        let Some(colors) = palette(ct) else { continue };
        let Some(content) = find_mut(page, &|e| e.transform == target) else { continue };
        content.children.clear(); // drop the baked placeholder swatches
        let (sw, sh, per_row) = (92.0f32, 92.0f32, 8usize);
        for (i, col) in colors.iter().enumerate() {
            let salt = MADE | (((0x400 + n * 64 + i) as i64) << 36);
            let mut c = fresh(&template, salt);
            c.active = true;
            // set the swatch's Background colour, and its select action, from the cloned ids
            let img = c.walk().iter().find_map(|(_, k)| k.impls().iter().find_map(|(cl, d)| cl.ends_with("/BackgroundColorImageImpl").then(|| pptr(d, "image")).flatten()));
            let btn = c.walk().iter().find_map(|(_, k)| k.impls().iter().find_map(|(cl, d)| cl.ends_with("/PrimaryToggleButtonImpl").then(|| pptr(d, "button")).flatten()));
            if let Some(id) = img {
                set_color(&mut c, id, *col);
            }
            if let Some(b) = btn {
                // colour actions carry the real colour as hex; outfitType 1000 = skin, 1001/1002 = hair
                actions.insert(b, Action::WearItem(1000 + ct, format!("#{:02x}{:02x}{:02x}", col[0], col[1], col[2])));
            }
            let (col_i, row_i) = (i % per_row, i / per_row);
            let r = &mut c.rect;
            (r.anchor_min, r.anchor_max, r.pivot) = ([0.0, 1.0], [0.0, 1.0], [0.0, 1.0]);
            r.size_delta = [sw, sh];
            r.anchored_position = [24.0 + col_i as f32 * (sw + 16.0), -24.0 - row_i as f32 * (sh + 16.0)];
            content.children.push(c);
        }
    }
}

/// Sets the `m_Color` of an Image component (by id) in `root` — like `set_text`, but for a colour.
fn set_color(root: &mut Element, id: i64, c: [u8; 3]) {
    let col = strukt(vec![
        ("r", Value::Float(c[0] as f64 / 255.0)),
        ("g", Value::Float(c[1] as f64 / 255.0)),
        ("b", Value::Float(c[2] as f64 / 255.0)),
        ("a", Value::Float(1.0)),
    ]);
    if let Some(e) = find_mut(root, &|e| has_component(e, id)) {
        for comp in e.components.iter_mut().filter(|comp| comp.id == id) {
            set(&mut comp.value, "m_Color", col.clone());
        }
    }
}

/// "CurlyHelmetHair"/"Blazer_Shirt" -> "Curly Helmet" / "Blazer": split camel case, drop a trailing slot word.
fn prettify(s: &str) -> String {
    let s = s.strip_suffix("_Shirt").or_else(|| s.strip_suffix("_Leg")).or_else(|| s.strip_suffix("_Shoe")).or_else(|| s.strip_suffix("_Hat")).unwrap_or(s);
    let s = s.strip_suffix("Hair").unwrap_or(s);
    let mut out = String::new();
    for (i, ch) in s.char_indices() {
        if i > 0 && ch.is_uppercase() && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push(ch);
    }
    out.trim().to_string()
}

/// A card (CardModelController impls) made from a list's item template: primary and secondary text, a badge text
/// (empty: no badge), its primary button doing `action`; the parts a card without them hides are hidden (no secondary
/// text or icon, no badge icon, not highlighted, not a junior account).
fn card(template: &Element, salt: i64, primary: &str, secondary: &str, badge: &str, action: Action, actions: &mut HashMap<i64, Action>) -> Element {
    let mut card = fresh(template, salt);
    (card.active, card.name) = (true, format!("{primary} card"));
    let (mut sets, mut hides, mut shows) = (vec![], vec![], vec![]);
    for (_, e) in card.walk() {
        for (class, d) in e.impls() {
            match class.rsplit('/').next().unwrap_or(class) {
                "PrimaryTextImpl" => sets.extend(pptr(d, "textMeshProUGUI").map(|t| (t, primary.to_string()))),
                "SecondaryTextImpl" => sets.extend(pptr(d, "textMeshProUGUI").map(|t| (t, secondary.to_string()))),
                "HideIfNoSecondaryTextImpl" if secondary.is_empty() => hides.extend(pptr(d, "hideable")),
                "HideIfNoBadgeTextAndIconImpl" | "HideIfNoBadgeTextImpl" if badge.is_empty() => hides.extend(pptr(d, "hideable")),
                "HideIfNoBadgeTextAndIconImpl" | "HideIfNoBadgeTextImpl" => shows.extend(pptr(d, "hideable")),
                "BadgeTextImpl" => sets.extend(pptr(d, "textMeshProUGUI").map(|t| (t, badge.to_string()))),
                "ShowIfJuniorAccountImpl" | "ShowIfJuniorForMetaEmptyImpl" | "HideIfNoSecondaryIconImpl" | "HideIfNoBadgeIconImpl"
                | "HideIfNotHighlightedImpl" | "HidePublishStateDataTokenImpl" => hides.extend(pptr(d, "hideable")),
                "PrimaryToggleButtonImpl" | "PrimaryButtonImpl" => actions.extend(pptr(d, "button").map(|b| (b, action.clone()))),
                _ => {}
            }
        }
    }
    for (t, text) in sets {
        set_text(&mut card, t, &text);
    }
    for h in hides {
        show(&mut card, h, false);
    }
    for h in shows {
        show(&mut card, h, true);
    }
    card
}

/// A virtualized room list (`list`: its VirtualizedScrollRectList component): a card per room from its item template,
/// in rows of `layoutRowsOrColumns` cells of `cellSize`, `itemSpacing` apart, inside `contentPadding`, top left first;
/// the content grows to hold them; a room's player count is its badge ("3 here"). A click on a card goes to its room
/// (INFERRED shortcut: the game opens the room's details page, whose Play button goes there).
fn room_cards(page: &mut Element, list: i64, rooms: &[RoomInfo], actions: &mut HashMap<i64, Action>) {
    let Some(holder) = find_mut(page, &|e| has_component(e, list)) else { return };
    let Some(template) = holder.children.iter().find(|c| c.name.ends_with(TEMPLATE)).cloned() else { return };
    let Some(v) = holder.components.iter().find(|c| c.id == list).map(|c| c.value.clone()) else { return };
    let num = |n: &str, d: f32| v.path(n).and_then(Value::as_f32).unwrap_or(d);
    let columns = v.get("layoutRowsOrColumns").and_then(Value::as_i64).unwrap_or(1).max(1) as usize;
    let (w, h, gap) = (num("cellSize/x", 300.0), num("cellSize/y", 260.0), num("itemSpacing", 0.0));
    let (left, top, bottom) = (num("contentPadding/Left", 0.0), num("contentPadding/Top", 0.0), num("contentPadding/Bottom", 0.0));
    let scroll = v.get("scrollRect").and_then(Value::as_pptr).map(|p| p.path_id);
    let content = holder.components.iter().find(|c| Some(c.id) == scroll).and_then(|c| pptr(&c.value, "m_Content"));
    let Some(content) = find_mut(holder, &|e| Some(e.transform) == content) else { return };
    for (i, room) in rooms.iter().enumerate() {
        let badge = room.players.map_or(String::new(), |n| format!("{n} here"));
        let mut c = card(&template, MADE | (0x40 + i as i64) << 40, &room.name, &room.description, &badge, Action::GoToRoom(room.name.clone(), room.scene.clone()), actions);
        let (col, row) = ((i % columns) as f32, (i / columns) as f32);
        let r = &mut c.rect;
        (r.anchor_min, r.anchor_max, r.pivot, r.size_delta) = ([0.0, 1.0], [0.0, 1.0], [0.0, 1.0], [w, h]);
        r.anchored_position = [left + col * (w + gap), -(top + row * (h + gap))];
        content.children.push(c);
    }
    let rows = rooms.len().div_ceil(columns) as f32;
    content.rect.size_delta[1] = content.rect.size_delta[1].max(top + rows * (h + gap) - gap + bottom);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::prefab::{Component, RectRules};

    #[test]
    fn the_router_stacks_pages_and_tabs_replace_the_top() {
        let mut r = Router::default();
        assert_eq!((r.top(), r.back_stack()), (None, 0));
        r.open(ESCAPE);
        assert!(!r.back(), "the escape menu stays when there is nothing to go back to");
        r.open(CONTROLS);
        r.tab(VISUALS);
        assert_eq!((r.top(), r.back_stack()), (Some(VISUALS), 1));
        r.open(LOGIN);
        assert!(r.back());
        assert_eq!(r.top(), Some(VISUALS));
        assert!(r.back());
        assert_eq!((r.top(), r.back_stack()), (Some(ESCAPE), 0));
        r.close();
        assert_eq!(r.top(), None);
    }

    fn s(fields: Vec<(&str, Value)>) -> Value {
        Value::Struct { type_name: "".into(), fields: fields.into_iter().map(|(k, v)| (k.into(), v)).collect() }
    }

    fn ptr(id: i64) -> Value {
        Value::PPtr(PPtr { file_id: 0, path_id: id })
    }

    /// A controller component holding one impl.
    fn controller(id: i64, class: &str, data: Value) -> Component {
        let refs = s(vec![("RefIds", Value::Array(vec![s(vec![("rid", Value::Int(0)), ("type", s(vec![("class", Value::String(class.into()))])), ("data", data)])]))]);
        Component { class: "RRUI.Data.X".into(), value: s(vec![("implementation", s(vec![("rid", Value::Int(0))])), ("references", refs)]), file: None, id, enabled: true, themed: Default::default() }
    }

    fn comp(id: i64, class: &str) -> Component {
        Component { class: class.into(), value: s(vec![]), file: None, id, enabled: true, themed: Default::default() }
    }

    fn el(name: &str, components: Vec<Component>, children: Vec<Element>) -> Element {
        Element { name: name.into(), active: true, id: 0, transform: 0, rect: RectRules::default(), components, children }
    }

    #[test]
    fn impls_become_actions_visibility_and_settings() {
        let guid = Value::Bytes((0..16).map(|i| u8::from_str_radix(&ROUTES[0].0[i * 2..i * 2 + 2], 16).unwrap()).collect());
        let back = el("back", vec![comp(10, "RRUI.MultiGraphicButton"), comp(11, "RRUI.Hideable"),
            controller(12, "BrowserModelController/NavigateBackButtonImpl", s(vec![("button", ptr(10))])),
            controller(13, "BrowserModelController/HideIfBackStackEmptyImpl", s(vec![("hideable", ptr(11))]))], vec![]);
        let settings = el("settings", vec![comp(20, "RRUI.MultiGraphicButton"),
            controller(21, "WatchModelController/WatchTabsImpl", s(vec![("button", ptr(20)), ("defaultUri", s(vec![("route", Value::Array(vec![s(vec![("viewGuid", s(vec![("bytes", guid)]))])]))]))]))], vec![]);
        let invert = el("invert", vec![comp(30, "RRUI.MultiGraphicButton"), comp(31, "RRUI.Theme.SwitchTheme"),
            controller(32, "SettingsModelController/InvertCameraToggleImpl", s(vec![("button", ptr(30)), ("toggleTheme", ptr(31)), ("Horizontal", Value::Int(0))]))], vec![]);
        let page = el("page", vec![], vec![back, settings, invert]);
        let mut st = Settings::default();
        st.set("invert_y", 1.0);
        let scroll = HashMap::new();
        let ctx = Ctx { page: ESCAPE, back_stack: 0, settings: &st, account: "me", typed: "", focused: false, scroll: &scroll, choosing: None, rooms: &[], item_card: None };
        let mut w = Wiring::default();
        // the theme is not needed to see the wiring: build's steps before it
        let mut hides = vec![];
        for (_, e) in page.walk() {
            for (class, d) in e.impls() {
                if let (Some(h), Some(x)) = (pptr(d, "hideable"), hidden(class, d, &ctx)) {
                    hides.push((h, x));
                }
                if let Some(k) = setting(class, d) {
                    w.switches.push((pptr(d, "toggleTheme").unwrap(), k));
                }
            }
        }
        assert_eq!(hides, [(11, true)]);
        assert_eq!(w.switches, [(31, "invert_y".to_string())]);
        assert_eq!(routes::uri(&page.children[1].impls()[0].1), [ROUTES[0].0]);
        assert_eq!(page_of(ROUTES[0].0), Some(CONTROLS));
        assert_eq!(setting("SettingsModelController/AutoSprintToggleImpl", &s(vec![])).as_deref(), Some("auto_sprint"));
        assert_eq!(setting("SettingsModelController/VolumeSliderImpl", &s(vec![("Type", Value::Int(3))])).as_deref(), Some("volume_music"));
        let pitch = s(vec![("button", ptr(1)), ("toggleTheme", ptr(2)), ("Mode", Value::UInt(2))]);
        assert_eq!(radio("SettingsModelController/VoicePitchRadioToggleImpl", &pitch), Some(("voice_pitch".to_string(), 2.0)));
        assert_eq!(radio("SettingsModelController/GraphicsQualityToggleImpl", &s(vec![("defaultQualityMapping", Value::Int(3))])), Some(("graphics_quality".to_string(), 3.0)));
        assert_eq!(radio("SettingsModelController/AutoSprintToggleImpl", &pitch), None);
        let lvl = s(vec![("textMeshProUGUI", ptr(1)), ("prefix", Value::String("Lvl ".into()))]);
        assert_eq!(account_text("AccountModelController/LevelTextImpl", &lvl, "me").as_deref(), Some("Lvl 1"));
        assert_eq!(account_text("AccountModelController/DisplayNameTextImpl", &s(vec![]), "me").as_deref(), Some("me"));
        assert_eq!(account_text("AccountModelController/PresenceTextImpl", &s(vec![]), "me"), None);
    }

    #[test]
    fn copies_get_their_own_ids_and_references() {
        let mut a = el("a", vec![comp(5, "X")], vec![]);
        a.components[0].value = s(vec![("self", ptr(5)), ("outside", ptr(99))]);
        (a.id, a.transform) = (6, 7);
        let b = fresh(&a, 1 << 40);
        assert_eq!((b.id, b.transform, b.components[0].id), (6 ^ 1 << 40, 7 ^ 1 << 40, 5 ^ 1 << 40));
        assert_eq!(pptr(&b.components[0].value, "self"), Some(5 ^ 1 << 40));
        assert_eq!(pptr(&b.components[0].value, "outside"), Some(99));
    }

    #[test]
    fn a_slider_shows_its_value() {
        let mut slider = comp(1, "RRUI.InputFieldSlider");
        slider.value = s(vec![("m_MinValue", Value::Float(0.0)), ("m_MaxValue", Value::Float(100.0)), ("m_FillRect", ptr(2)), ("m_HandleRect", ptr(3)), ("currentValueLabel", ptr(4))]);
        let mut fill = el("fill", vec![], vec![]);
        fill.transform = 2;
        let mut handle = el("handle", vec![], vec![]);
        handle.transform = 3;
        let mut text = comp(4, "TMPro.TextMeshProUGUI");
        text.value = s(vec![("m_text", Value::String("50".into()))]);
        let mut page = el("slider", vec![slider], vec![fill, handle, el("label", vec![text], vec![])]);
        show_slider(&mut page, 1, 25.0, None, "25");
        assert_eq!((page.children[0].rect.anchor_min, page.children[0].rect.anchor_max), ([0.0, 0.0], [0.25, 1.0]));
        assert_eq!((page.children[1].rect.anchor_min, page.children[1].rect.anchor_max), ([0.25, 0.0], [0.25, 1.0]));
        assert_eq!(page.children[2].components[0].value.get("m_text").and_then(Value::as_str), Some("25"));
    }
}
