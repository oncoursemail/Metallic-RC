//! The game's menus (docs/agents/rc-spec/ui.md in the private notes): uGUI page prefabs read from the owner's install
//! and laid out the way Unity does, ready to draw.
//!
//! - [`prefab`]: a page prefab as a tree of elements, each with its rect rules and its components;
//! - [`layout`]: Unity's RectTransform and layout-group rules, which place the elements;
//! - [`theme`]: the palette theme, which colours and sizes the graphics its components point at;
//! - [`paint`]: what each element draws (sprites from the atlases, the TTFs behind the TMP fonts, colours);
//! - [`view`]: the painted page as Bevy UI nodes;
//! - [`input`]: the pointer on a page: hit tests, selectable states and their tints;
//! - [`routes`]: route GUIDs to pages, page paths and pages nested in their hosts' Routers;
//! - [`menu`]: the escape menu's pages, what their controllers do and which page is open.
pub mod input;
pub mod layout;
pub mod menu;
pub mod paint;
pub mod prefab;
pub mod routes;
pub mod theme;
pub mod view;
