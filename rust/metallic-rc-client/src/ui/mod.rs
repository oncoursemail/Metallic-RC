//! The game's menus (docs/agents/rc-spec/ui.md in the private notes): uGUI page prefabs read from the owner's install
//! and laid out the way Unity does, ready to draw.
//!
//! - [`prefab`]: a page prefab as a tree of elements, each with its rect rules and its components;
//! - [`layout`]: Unity's RectTransform and layout-group rules, which place the elements.
pub mod layout;
pub mod prefab;
