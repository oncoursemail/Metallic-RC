//! Loads a menu page from the installed game, lays it out on a 1920x1080 canvas and prints the tree.
//! `cargo run --release -p metallic-rc-client --example ui_page [address] [game dir]`, `address` defaulting to the
//! escape menu; an address without parentheses lists the pages whose address contains it.
use metallic_rc_client::room::{self, Library};
use metallic_rc_client::ui::{layout, prefab};

fn main() {
    let address = std::env::args().nth(1).unwrap_or_else(|| "(EscapeMenu_standalone)".into());
    let game = std::env::args().nth(2).map(std::path::PathBuf::from)
        .or_else(|| room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()))
        .expect("no game folder");
    let dir = room::bundle_dir(&game);
    let t = std::time::Instant::now();
    let pages = prefab::Pages::open(&dir).unwrap();
    let mut lib = Library::new(&dir).unwrap();
    println!("catalog and index in {:?}", t.elapsed());
    if !address.starts_with('(') {
        for a in pages.search(&address) {
            println!("{a}");
        }
        return;
    }
    let t = std::time::Instant::now();
    let page = prefab::load(&mut lib, &pages, &dir, &address).unwrap();
    let laid = layout::lay_out(&page, [1920.0, 1080.0], &mut layout::Rough);
    println!("{} elements, {} drawn, in {:?}", page.walk().len(), laid.boxes.len(), t.elapsed());
    let all = page.walk();
    for (index, depth, b) in &laid.boxes {
        let e = all[*index].1;
        let classes: Vec<&str> = e.components.iter().filter(|c| c.enabled)
            .map(|c| c.class.rsplit('.').next().unwrap_or(&c.class)).collect();
        let text = e.component("TMPro.TextMeshProUGUI").and_then(|c| c.value.get("m_text")).and_then(|v| v.as_str())
            .map(|s| format!(" \"{}\"", s.chars().take(30).collect::<String>().replace('\n', " "))).unwrap_or_default();
        println!("{:indent$}{} [{:.0},{:.0} {:.0}x{:.0}] {}{text}", "", e.name, b.x, b.y, b.w, b.h, classes.join(" "), indent = depth * 2);
    }
}
