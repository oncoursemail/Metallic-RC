//! Dump the type tree for a class id from a bundle (authoritative Unity 6 field layout).
//! usage: rr-typetree <bundle> <class_id>
use unity_assets::*;

fn main() {
    let bundle = std::env::args().nth(1).expect("bundle");
    let class: i32 = std::env::args().nth(2).expect("class_id").parse().unwrap();
    let b = Bundle::open(std::path::Path::new(&bundle)).unwrap();
    let files = b.serialized_files().unwrap();
    for (name, sf) in &files {
        for t in &sf.types {
            if t.class_id == class {
                if let Some(tree) = &t.tree {
                    println!("# {name}: class {class} ({})", sf.unity_version);
                    print!("{}", tree.dump());
                    return;
                }
            }
        }
    }
    println!("no type tree for class {class} in {bundle}");
}
