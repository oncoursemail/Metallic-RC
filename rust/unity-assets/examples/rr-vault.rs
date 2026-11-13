//! Read a ColorVault's real colour pool via MonoFields (the bundle's MonoBehaviour tree is stripped). Layout from
//! the decompile lane: ColorVault { Colors: List<Entry> } ; Entry { ColorToGuidSerializable, bool isDefaultEligible } ;
//! ColorToGuidSerializable { FriendlyName:string, GuidString:string, Color, SecondaryColor, EmissiveColor, Gloss:float }.
//! The default skin/hair colour is GetRandomDefault() over the entries flagged isDefaultEligible.
//! usage: rr-vault <bundle-file> <pathid> [label]
use unity_assets::*;

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("bundle file"));
    let want: i64 = std::env::args().nth(2).expect("path_id").parse().expect("i64");
    let label = std::env::args().nth(3).unwrap_or_default();
    let b = Bundle::open(&path).expect("bundle");
    let sfs = b.serialized_files().expect("sfs");
    let Some((_, sf, o)) = sfs.iter().find_map(|(c, sf)| Some((c, sf, sf.object(want)?))) else {
        println!("path_id {want} not found");
        return;
    };
    // The bundle reports no Unity version, so parse the stripped MonoBehaviour by hand (same layout MonoFields uses).
    let mut r = Reader::new(sf.object_bytes(o), sf.big_endian);
    (r.i32().unwrap(), r.i64().unwrap()); // m_GameObject
    r.u8().unwrap();
    r.align(4); // m_Enabled
    (r.i32().unwrap(), r.i64().unwrap()); // m_Script
    r.aligned_string().unwrap(); // m_Name
    let color4 = |r: &mut Reader| [r.f32().unwrap_or(0.0), r.f32().unwrap_or(0.0), r.f32().unwrap_or(0.0), r.f32().unwrap_or(0.0)];
    let count = r.len_prefix(1).expect("Colors count");
    println!("{label} vault {want}: {count} colours");
    let mut defaults = Vec::new();
    for i in 0..count {
        // ColorToGuidSerializable
        let friendly = r.aligned_string().unwrap_or_default();
        let _guid = r.aligned_string().unwrap_or_default();
        let color = color4(&mut r);
        let _secondary = color4(&mut r);
        let _emissive = color4(&mut r);
        let _gloss = r.f32().unwrap_or(0.0);
        // Entry.isDefaultEligible
        let is_default = r.u8().unwrap_or(0) != 0;
        r.align(4);
        let sane = color.iter().all(|&x| (-0.01..=1.5).contains(&x));
        let rgba255 = [(color[0] * 255.0) as u8, (color[1] * 255.0) as u8, (color[2] * 255.0) as u8, (color[3] * 255.0) as u8];
        println!(
            "  #{i:<2} {:<24} rgba=[{:.3} {:.3} {:.3} {:.3}] = {:?}  default={}{}",
            friendly, color[0], color[1], color[2], color[3], rgba255, is_default, if sane { "" } else { "  <-- INSANE" }
        );
        if is_default {
            defaults.push((friendly, color, rgba255));
        }
    }
    println!("\n{label} DEFAULT-ELIGIBLE pool ({}):", defaults.len());
    for (name, c, c255) in &defaults {
        println!("  {name:<24} rgba=[{:.3} {:.3} {:.3} {:.3}] = {:?}", c[0], c[1], c[2], c[3], c255);
    }
}
