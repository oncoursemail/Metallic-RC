//! Hex-dump the first object(s) of a given class id from a plain SerializedFile.
//! usage: rr-dump <file> <class_id> [count] [max_bytes]
use unity_assets::*;

fn main() {
    let mut a = std::env::args().skip(1);
    let path = a.next().expect("file");
    let class: i32 = a.next().expect("class_id").parse().unwrap();
    let count: usize = a.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let maxb: usize = a.next().and_then(|s| s.parse().ok()).unwrap_or(320);
    let sf = SerializedFile::parse(std::fs::read(&path).unwrap()).unwrap();
    let mut shown = 0;
    for o in sf.objects.iter().filter(|o| o.class_id == class) {
        let b = sf.object_bytes(o);
        let name = sf.object_name(o).unwrap_or_default();
        println!("\n=== class {class} path_id {} size {} name {name:?} ===", o.path_id, b.len());
        for (i, chunk) in b.chunks(16).take(maxb / 16).enumerate() {
            let hex: Vec<String> = chunk.iter().map(|x| format!("{x:02x}")).collect();
            let asc: String = chunk.iter().map(|&x| if (0x20..0x7f).contains(&x) { x as char } else { '.' }).collect();
            println!("{:4}: {:<48} {}", i * 16, hex.join(" "), asc);
        }
        shown += 1;
        if shown >= count {
            break;
        }
    }
}
