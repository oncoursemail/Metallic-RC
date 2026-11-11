//! Offscreen verification renderer: loads a built-in room with the real loader and rasterizes it to a PNG, so a
//! scene's geometry + materials can be checked without a GPU or window.
//!
//! usage: rr-rasterize <scene> <out.png> [--game-dir DIR] [--size WxH] [--eye x,y,z] [--target x,y,z]
use bevy::math::{Mat3, Mat4, Vec3};
use metallic_rc_client::{levels, room::RoomData, world};
use std::path::PathBuf;

fn vec3(s: &str) -> Option<Vec3> {
    let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let scene = args.next().expect("scene name");
    let out = args.next().expect("out.png");
    let (mut game_dir, mut size, mut eye, mut target) = (None, (1280u32, 720u32), None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--game-dir" => game_dir = args.next().map(PathBuf::from),
            "--size" => {
                let s = args.next().unwrap_or_default();
                if let Some((w, h)) = s.split_once('x') {
                    size = (w.parse().unwrap(), h.parse().unwrap());
                }
            }
            "--eye" => eye = args.next().as_deref().and_then(vec3),
            "--target" => target = args.next().as_deref().and_then(vec3),
            _ => panic!("unknown arg {a}"),
        }
    }
    let game = game_dir.or_else(|| metallic_rc_client::room::GAME_DIRS.iter().map(PathBuf::from).find(|p| p.exists()))
        .expect("no game dir found; pass --game-dir");
    let data = game.join("RecRoom_Data");

    let t = std::time::Instant::now();
    let r = levels::load_scene(Some(&data), &scene).expect("no such scene in the decrypted corpus").expect("load failed");
    eprintln!("loaded {scene}: {} parts, {} materials, {} lights, {} colliders in {:.1}s",
        r.parts.len(), r.materials.len(), r.lights.len(), r.colliders.len(), t.elapsed().as_secs_f32());
    if !r.problems.is_empty() {
        eprintln!("problems ({}): {:?}", r.problems.len(), &r.problems[..r.problems.len().min(8)]);
    }

    let (lo, hi) = world::bounds(&r);
    let center = (lo + hi) * 0.5;
    let sz = hi - lo;
    let radius = sz.length() * 0.5;
    // default: a 3/4 overview from a high corner, looking at the room centre
    let eye = eye.unwrap_or(center + Vec3::new(0.9, 0.7, 0.9).normalize() * radius * 1.7);
    let target = target.unwrap_or(center);
    eprintln!("bounds {lo:?}..{hi:?}  eye {eye:?} target {target:?}");

    let (w, h) = size;
    let png = render(&r, eye, target, w, h);
    let file = std::fs::File::create(&out).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&png).unwrap();
    eprintln!("wrote {out} ({w}x{h})");
}

/// Rasterizes the room to an RGB8 buffer with a z-buffer and a simple head-light shade.
fn render(r: &RoomData, eye: Vec3, target: Vec3, w: u32, h: u32) -> Vec<u8> {
    let aspect = w as f32 / h as f32;
    let fov = 55f32.to_radians();
    let view = Mat4::look_at_rh(eye, target, Vec3::Y);
    let proj = Mat4::perspective_rh(fov, aspect, 0.05, 10_000.0);
    let vp = proj * view;
    let (wf, hf) = (w as f32, h as f32);

    // sky gradient background
    let mut color = vec![0u8; (w * h * 3) as usize];
    for y in 0..h {
        let t = y as f32 / hf;
        let c = [(120.0 + 90.0 * (1.0 - t)) as u8, (150.0 + 80.0 * (1.0 - t)) as u8, (190.0 + 60.0 * (1.0 - t)) as u8];
        for x in 0..w {
            let i = ((y * w + x) * 3) as usize;
            color[i..i + 3].copy_from_slice(&c);
        }
    }
    let mut depth = vec![f32::INFINITY; (w * h) as usize];

    for p in &r.parts {
        let mat = &r.materials[p.material];
        let normal_mat = Mat3::from_mat4(p.world);
        // clip-space positions of each vertex
        let clip: Vec<(bevy::math::Vec4, Vec3, [f32; 2])> = (0..p.positions.len()).map(|i| {
            let world = p.world.transform_point3(Vec3::from(p.positions[i]));
            let n = normal_mat.mul_vec3(Vec3::from(*p.normals.get(i).unwrap_or(&[0.0, 1.0, 0.0]))).normalize_or_zero();
            (vp * world.extend(1.0), n, *p.uvs.get(i).unwrap_or(&[0.0, 0.0]))
        }).collect();

        for tri in p.indices.chunks_exact(3) {
            let (a, b, c) = (&clip[tri[0] as usize], &clip[tri[1] as usize], &clip[tri[2] as usize]);
            // near-plane / behind-camera cull
            if a.0.w <= 0.05 || b.0.w <= 0.05 || c.0.w <= 0.05 {
                continue;
            }
            let sp = |v: &bevy::math::Vec4| {
                let ndc = v.truncate() / v.w;
                (((ndc.x * 0.5 + 0.5) * wf), ((1.0 - (ndc.y * 0.5 + 0.5)) * hf), 1.0 / v.w)
            };
            let (pa, pb, pc) = (sp(&a.0), sp(&b.0), sp(&c.0));
            let (minx, maxx) = (pa.0.min(pb.0).min(pc.0).floor().max(0.0) as u32, pa.0.max(pb.0).max(pc.0).ceil().min(wf) as u32);
            let (miny, maxy) = (pa.1.min(pb.1).min(pc.1).floor().max(0.0) as u32, pa.1.max(pb.1).max(pc.1).ceil().min(hf) as u32);
            let area = (pb.0 - pa.0) * (pc.1 - pa.1) - (pb.1 - pa.1) * (pc.0 - pa.0);
            if area.abs() < 1e-6 {
                continue;
            }
            let inv_area = 1.0 / area;
            // flat face normal in view direction, for the head-light shade
            let n = (a.1 + b.1 + c.1).normalize_or_zero();
            let shade = 0.35 + 0.65 * n.dot((eye - target).normalize_or_zero()).abs();
            for py in miny..maxy {
                for px in minx..maxx {
                    let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                    let w0 = ((pb.0 - fx) * (pc.1 - fy) - (pb.1 - fy) * (pc.0 - fx)) * inv_area;
                    let w1 = ((pc.0 - fx) * (pa.1 - fy) - (pc.1 - fy) * (pa.0 - fx)) * inv_area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let z = w0 * pa.2 + w1 * pb.2 + w2 * pc.2; // 1/w, larger = nearer
                    let di = (py * w + px) as usize;
                    if z <= 1.0 / depth[di] {
                        continue;
                    }
                    depth[di] = 1.0 / z;
                    // perspective-correct uv
                    let uv = [
                        (w0 * a.2[0] * pa.2 + w1 * b.2[0] * pb.2 + w2 * c.2[0] * pc.2) / z,
                        (w0 * a.2[1] * pa.2 + w1 * b.2[1] * pb.2 + w2 * c.2[1] * pc.2) / z,
                    ];
                    let rgb = shade_pixel(mat, uv);
                    let ci = di * 3;
                    color[ci] = ((rgb[0] * shade) * 255.0).clamp(0.0, 255.0) as u8;
                    color[ci + 1] = ((rgb[1] * shade) * 255.0).clamp(0.0, 255.0) as u8;
                    color[ci + 2] = ((rgb[2] * shade) * 255.0).clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    color
}

/// Approximate the material's surface colour (sRGB 0..1) at a uv: the base texture (scaled/offset like the shader),
/// tinted by the material colour. Good enough to tell a real look from a grey stand-in.
fn shade_pixel(mat: &metallic_rc_client::room::RoomMaterial, uv: [f32; 2]) -> [f32; 3] {
    let tint = [mat.color[0], mat.color[1], mat.color[2]];
    let Some((tw, th, px)) = &mat.texture else { return tint };
    let (tw, th) = (*tw as i32, *th as i32);
    let su = uv[0] * mat.uv_scale[0] + mat.uv_offset[0];
    let sv = uv[1] * mat.uv_scale[1] + mat.uv_offset[1];
    let wrap = |f: f32, n: i32| {
        let m = ((f.fract() + 1.0).fract() * n as f32) as i32;
        m.clamp(0, n - 1)
    };
    let (x, y) = (wrap(su, tw), wrap(sv, th));
    let i = ((y * tw + x) * 4) as usize;
    if i + 2 >= px.len() {
        return tint;
    }
    let base = [px[i] as f32 / 255.0, px[i + 1] as f32 / 255.0, px[i + 2] as f32 / 255.0];
    // masked AG shaders keep the texture's own colour; others multiply by the tint
    if mat.masked {
        base
    } else {
        [base[0] * tint[0], base[1] * tint[1], base[2] * tint[2]]
    }
}
