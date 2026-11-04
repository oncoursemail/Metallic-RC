//! Paints a menu page the way the client's menu does and prints what is drawn: each element's box, fills, texts and
//! selectable, and the page's actions.
//! `cargo run --release -p metallic-rc-client --example menu_dump [address] [game dir]`
use std::collections::HashMap;

use metallic_rc_client::room;
use metallic_rc_client::settings::Settings;
use metallic_rc_client::ui::menu;
use metallic_rc_client::ui::paint::{Drawn, Paint};

fn main() {
    let address = std::env::args().nth(1).unwrap_or_else(|| menu::ESCAPE.into());
    let game = std::env::args().nth(2).map(std::path::PathBuf::from)
        .or_else(|| room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()))
        .expect("no game folder");
    let painter = menu::Painter::start(room::bundle_dir(&game));
    painter.ask(menu::Request { page: address, back_stack: 1, settings: Settings::default(), account: "Player".into(), typed: String::new(),
        focused: false, scroll: HashMap::new(), choosing: None, server: String::new() });
    let p = loop {
        if let Some(r) = painter.take() {
            break r.unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    fn show(d: &Drawn, depth: usize) {
        let r = d.rect;
        let i = d.interact.as_ref().map(|i| format!(" <selectable {} {:?} target {:?} extra {:?}>", i.id, i.kind, i.target, i.extra_tints)).unwrap_or_default();
        println!("{:depth$}{} [{:.0},{:.0} {:.0}x{:.0}]{}{}", "", d.name, r.x, r.y, r.w, r.h, if d.raycast { " ray" } else { "" }, i, depth = depth * 2);
        for p in &d.paint {
            match p {
                Paint::Fill { color, sprite, graphic, gradient, .. } => println!("{:depth$}  fill #{graphic} {color:?} sprite {:?} gradient {gradient:?}", "", sprite.as_ref().map(|s| (s.width, s.height)), depth = depth * 2),
                Paint::Text { text, color, size, graphic, .. } => println!("{:depth$}  text #{graphic} {text:?} {color:?} {size}", "", depth = depth * 2),
                Paint::Tiles { .. } => println!("{:depth$}  tiles", "", depth = depth * 2),
            }
        }
        for c in &d.children {
            show(c, depth + 1);
        }
    }
    show(&p.drawn, 0);
    let mut actions: Vec<_> = p.actions.iter().collect();
    actions.sort_by_key(|(k, _)| **k);
    for (id, a) in actions {
        println!("action {id}: {a:?}");
    }
}
