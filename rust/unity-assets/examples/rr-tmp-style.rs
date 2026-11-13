//! Validate reading real TMP_Text style (m_fontColor, m_fontSize, m_textAlignment) from stripped scenes via
//! MonoFields, using the decompile lane's authoritative field order. Self-check: m_fontColor32 (4 bytes) must equal
//! m_fontColor (4 floats) x255 — same colour, two encodings — so if they agree the offsets are right.
//! usage: rr-tmp-style <level-file>
use std::collections::HashMap;
use unity_assets::*;

fn main() {
    let level = std::path::PathBuf::from(std::env::args().nth(1).expect("level file"));
    let ldir = level.parent().unwrap().to_path_buf();
    let sf = SerializedFile::parse(std::fs::read(&level).unwrap()).unwrap();
    let mut sib: HashMap<String, SerializedFile> = HashMap::new();
    for ext in &sf.externals {
        let n = ext.path.rsplit('/').next().unwrap_or(&ext.path).to_string();
        if !sib.contains_key(&n) {
            if let Some(f) = std::fs::read(ldir.join(&n)).ok().and_then(|d| SerializedFile::parse(d).ok()) {
                sib.insert(n, f);
            }
        }
    }
    let class_of = |o: &ObjectInfo| -> Option<String> {
        let p = level::mono_script_ref(&sf, o).ok()?;
        if p.file_id == 0 {
            return level::mono_script_name(&sf, sf.object(p.path_id)?).ok().map(|(c, _)| c);
        }
        let ext = sf.externals.get(p.file_id as usize - 1)?;
        let f = sib.get(ext.path.rsplit('/').next().unwrap_or(&ext.path))?;
        level::mono_script_name(f, f.object(p.path_id)?).ok().map(|(c, _)| c)
    };

    let get_f = |b: &[u8], o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let (mut total, mut found, mut size_at148, mut size_at144) = (0, 0, 0, 0);
    let mut gaps: HashMap<i64, usize> = HashMap::new();
    let mut samples = Vec::new();
    for o in sf.objects.iter().filter(|o| o.class_id == classes::MONO_BEHAVIOUR) {
        if class_of(o).as_deref() != Some("TextMeshProUGUI") {
            continue;
        }
        total += 1;
        // find the byte offset just after m_text (header + Graphic base + m_text)
        let bytes = sf.object_bytes(o);
        let mut r = Reader::new(bytes, sf.big_endian);
        let after_text = (|| -> Option<usize> {
            (r.i32().ok()?, r.i64().ok()?); // m_GameObject
            r.u8().ok()?;
            r.align(4); // m_Enabled
            (r.i32().ok()?, r.i64().ok()?); // m_Script
            r.aligned_string().ok()?; // m_Name
            (r.i32().ok()?, r.i64().ok()?); // m_Material
            for _ in 0..4 {
                r.f32().ok()?; // m_Color
            }
            r.u8().ok()?;
            r.align(4); // m_RaycastTarget
            for _ in 0..4 {
                r.f32().ok()?; // m_RaycastPadding
            }
            r.u8().ok()?;
            r.align(4); // m_Maskable
            r.aligned_string().ok()?; // m_text
            Some(r.pos())
        })();
        let Some(start) = after_text else { continue };
        // scan 4-aligned offsets for the fontColor32 (4 bytes) + fontColor (4 floats) pair: alpha high and
        // byte==float*255 for all channels -> that is m_fontColor, unambiguously
        let mut hit = None;
        let mut off = (start + 3) & !3;
        while off + 20 <= bytes.len() && off < start + 160 {
            let c32 = [bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]];
            let col = [get_f(bytes, off + 4), get_f(bytes, off + 8), get_f(bytes, off + 12), get_f(bytes, off + 16)];
            let sane = col.iter().all(|&x| (-0.01..=1.01).contains(&x));
            let consistent = (0..4).all(|k| ((c32[k] as f32) - col[k] * 255.0).abs() <= 2.0);
            if sane && consistent && c32[3] >= 200 && (c32[0] as u16 + c32[1] as u16 + c32[2] as u16) > 20 {
                hit = Some((off, c32, col));
                break;
            }
            off += 4;
        }
        if let Some((o2, c32, col)) = hit {
            found += 1;
            *gaps.entry((o2 - start) as i64).or_default() += 1;
            // forward from m_fontColor32 (anchor) is fixed-size; m_fontSize is at anchor+148 (each bool aligns) or
            // anchor+144 (the allowEmojis/tintAllSprites pair packs). Read both and count which is a sane font size.
            let size_at = |d: usize| if o2 + d + 4 <= bytes.len() { Some(get_f(bytes, o2 + d)) } else { None };
            let (sa, sb) = (size_at(148), size_at(144));
            let sane = |s: Option<f32>| s.is_some_and(|x| (6.0..=160.0).contains(&x));
            if sane(sa) {
                size_at148 += 1;
            }
            if sane(sb) {
                size_at144 += 1;
            }
            if samples.len() < 10 {
                samples.push(format!("gap={} fontColor=[{:.2} {:.2} {:.2} {:.2}] size@148={:?} size@144={:?}", o2 - start, col[0], col[1], col[2], col[3], sa.map(|x| (x * 10.0).round() / 10.0), sb.map(|x| (x * 10.0).round() / 10.0)));
            }
        }
    }
    println!("TextMeshProUGUI: {total}; m_fontColor pair found: {found}");
    println!("  m_fontSize sane at anchor+148: {size_at148}/{found};  at anchor+144: {size_at144}/{found}");
    println!("gap (bytes from after-m_text to m_fontColor32), by frequency:");
    let mut gv: Vec<_> = gaps.iter().collect();
    gv.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (g, c) in gv.iter().take(12) {
        println!("  gap {g}: {c} labels");
    }
    for s in &samples {
        println!("  {s}");
    }
}
