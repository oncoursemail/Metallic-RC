//! TEMP: extract FMOD Vorbis setup headers from UnityPlayer.dll and match them to the corpus clips'
//! setup_ids by CRC32 (self-validating length). `vorbis-extract <UnityPlayer.dll> <bundle-dir> <corpus>`
use std::path::Path;
use unity_assets::*;

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn u32le(b: &[u8], p: usize) -> u32 { u32::from_le_bytes([b[p], b[p+1], b[p+2], b[p+3]]) }
fn u64le(b: &[u8], p: usize) -> u64 { u64::from_le_bytes(b[p..p+8].try_into().unwrap()) }

fn get_tree(dir: &str, class: i32) -> Option<TypeTree> {
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "bundle").unwrap_or(false)).collect();
    files.sort();
    for f in &files { if let Ok(b) = Bundle::open(f) { if let Ok(sfs) = b.serialized_files() {
        for (_, sf) in &sfs { for t in &sf.types { if t.class_id == class { if let Some(tt) = &t.tree { return Some(tt.clone()); } } } }
    }}}
    None
}

fn vorbis_setup_id(fsb: &[u8]) -> Option<u32> {
    if fsb.len() < 0x3c || &fsb[..4] != b"FSB5" { return None; }
    let version = u32le(fsb, 4);
    let base = if version == 0 { 0x40 } else { 0x3c };
    let t = u64le(fsb, base);
    if t & 1 == 0 { return None; }
    let mut p = base + 8;
    loop {
        if p + 4 > fsb.len() { return None; }
        let c = u32le(fsb, p); p += 4;
        let more = c & 1;
        let size = ((c >> 1) & 0xFFFFFF) as usize;
        let typ = (c >> 25) & 0x7F;
        if typ == 0x0B && size >= 4 && p + 4 <= fsb.len() { return Some(u32le(fsb, p)); }
        p += size;
        if more == 0 { return None; }
    }
}

fn main() {
    let mut a = std::env::args().skip(1);
    let up_path = a.next().unwrap();
    let bdir = a.next().unwrap();
    let corpus = a.next().unwrap();

    // target setup_ids from corpus Vorbis clips
    let tree = get_tree(&bdir, classes::AUDIO_CLIP).expect("no AudioClip tree");
    let sf = SerializedFile::parse(std::fs::read(Path::new(&corpus).join("resources.assets")).unwrap()).unwrap();
    let res = std::fs::read(Path::new(&corpus).join("resources.resource")).unwrap();
    let mut targets = std::collections::BTreeSet::<u32>::new();
    for o in sf.objects.iter().filter(|o| o.class_id == classes::AUDIO_CLIP) {
        let mut r = Reader::new(sf.object_bytes(o), sf.big_endian);
        let Ok(v) = value::read(&tree, &sf.ref_types, &mut r) else { continue };
        let clip = AudioClip::from_value(&v).unwrap();
        if clip.compression != 1 { continue; }
        let (o2, n) = (clip.offset as usize, clip.size as usize);
        if o2 + n <= res.len() { if let Some(id) = vorbis_setup_id(&res[o2..o2+n]) { targets.insert(id); } }
    }
    println!("target setup_ids: {}", targets.len());

    // find 05 'vorbis' packets in UnityPlayer.dll
    let up = std::fs::read(&up_path).unwrap();
    let needle = b"\x05vorbis";
    let mut offs = Vec::new();
    let mut i = 0;
    while i + 7 <= up.len() { if &up[i..i+7] == needle { offs.push(i); i += 7; } else { i += 1; } }
    println!("05vorbis packets in UnityPlayer.dll: {}", offs.len());

    // brute-force: for each packet, find L (and start = off or off+7) whose crc32 hits a target
    let mut found = std::collections::BTreeMap::<u32, (usize, usize)>::new();
    for &off in &offs {
        let maxl = (offs.iter().filter(|&&x| x > off).min().map(|&x| x - off).unwrap_or(4096)).min(8192);
        'outer: for &start in &[off, off + 7] {
            for l in 8..maxl {
                if start + l > up.len() { break; }
                let c = crc32(&up[start..start + l]);
                if targets.contains(&c) {
                    found.entry(c).or_insert((start, l));
                    break 'outer;
                }
            }
        }
    }
    println!("\nMATCHED {}/{} setup_ids:", found.len(), targets.len());
    for (id, (start, l)) in &found { println!("  {id:#010x}: offset {start} len {l}"); }
    let missing: Vec<_> = targets.iter().filter(|t| !found.contains_key(t)).collect();
    println!("MISSING {}: {:?}", missing.len(), missing.iter().map(|x| format!("{x:#010x}")).collect::<Vec<_>>());

    // clip coverage of matched ids
    let mut covered = 0; let mut total_clips = 0;
    for o in sf.objects.iter().filter(|o| o.class_id == classes::AUDIO_CLIP) {
        let mut r = Reader::new(sf.object_bytes(o), sf.big_endian);
        let Ok(v) = value::read(&tree, &sf.ref_types, &mut r) else { continue };
        let clip = AudioClip::from_value(&v).unwrap();
        if clip.compression != 1 { continue; }
        let (o2, n) = (clip.offset as usize, clip.size as usize);
        if o2 + n <= res.len() { if let Some(id) = vorbis_setup_id(&res[o2..o2+n]) { total_clips += 1; if found.contains_key(&id) { covered += 1; } } }
    }
    println!("clip coverage: {covered}/{total_clips} Vorbis clips decodable with matched setups");

    // write local gitignored setup table: [u32 count][ {u32 id, u32 len, bytes} ... ]
    if let Ok(dir) = std::env::var("RR_VORBIS_OUT") {
        std::fs::create_dir_all(&dir).ok();
        let mut out = Vec::new();
        out.extend_from_slice(&(found.len() as u32).to_le_bytes());
        for (id, (start, l)) in &found {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&(*l as u32).to_le_bytes());
            out.extend_from_slice(&up[*start..*start + *l]);
        }
        let p = Path::new(&dir).join("vorbis_setups.bin");
        std::fs::write(&p, &out).unwrap();
        println!("wrote {} ({} setups, {} bytes)", p.display(), found.len(), out.len());

        // PROVE: decode a few Vorbis clips whose setup we have, to WAV
        let setups = VorbisSetups::load(Path::new(&dir)).unwrap();
        let outdir = Path::new(&dir).join("wav");
        std::fs::create_dir_all(&outdir).ok();
        let mut proved = 0;
        for o in sf.objects.iter().filter(|o| o.class_id == classes::AUDIO_CLIP) {
            if proved >= 3 { break; }
            let mut r = Reader::new(sf.object_bytes(o), sf.big_endian);
            let Ok(v) = value::read(&tree, &sf.ref_types, &mut r) else { continue };
            let clip = AudioClip::from_value(&v).unwrap();
            if clip.compression != 1 { continue; }
            let (o2, n) = (clip.offset as usize, clip.size as usize);
            if o2 + n > res.len() { continue; }
            match clip.decode(&res[..], Some(&setups)) {
                Ok(a) if a.samples.iter().any(|&s| s != 0) => {
                    let secs = a.samples.len() as f32 / (a.channels.max(1) * a.sample_rate) as f32;
                    let f = outdir.join(format!("{}.wav", clip.name.replace(['/', '|', '\\', ':'], "_")));
                    std::fs::write(&f, a.to_wav()).unwrap();
                    println!("  decoded {:?}: {} samples {}ch {:.2}s (declared {:.2}s) -> {}", clip.name, a.samples.len(), a.channels, secs, clip.length_seconds, f.display());
                    proved += 1;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
    }
}
