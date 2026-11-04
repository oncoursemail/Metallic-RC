//! A scripted pen for trying the Maker with two windows, by `METALLIC_MAKER_DEMO` (debug aid, like netobj's
//! `METALLIC_NET_DEBUG`). Times are seconds after the room's save was handled:
//! - `build` (the room's owner): 1 s takes the pen out, 1.5 s makes a row of shapes and props in front, 2 s lets
//!   everyone in the room build, 4 s picks up the first cube, 4-6 s carries it up and turns it, makes it blue and
//!   bigger, 6 s puts it down, 8 s saves the room.
//! - `move` (another player): 1 s takes the pen out, from 7 s tries (until allowed) to pick up the nearest maker
//!   object, carries it round for 4 s, puts it down and deletes the next nearest one.
use bevy::prelude::*;
use rr_net::objects::RpcTarget;

use super::{edit::{Change, ObjState}, hold_new, kit::{Kind, Shape, PROPS}, put_down, save, MadeQuery, Pen, Request, Sel};
use crate::netobj::{NetObjects, RoomLink};
use crate::player::{Player, View};

#[derive(Default)]
pub struct Script {
    mode: Option<String>,
    start: Option<f64>,
    done: u32,
}

#[allow(clippy::too_many_arguments)]
pub fn demo(
    time: Res<Time>,
    mut script: Local<Script>,
    mut pen: ResMut<Pen>,
    mut net: NetObjects,
    mut made: MadeQuery,
    player: Single<&Transform, (With<Player>, Without<super::Made>, Without<View>)>,
    link: Res<RoomLink>,
) {
    let mode = script.mode.get_or_insert_with(|| std::env::var("METALLIC_MAKER_DEMO").unwrap_or_default()).clone();
    if mode.is_empty() || !pen.loaded || link.get().is_none() {
        return;
    }
    let pen = &mut *pen;
    let now = time.elapsed_secs_f64();
    let t = now - *script.start.get_or_insert(now);
    let step = |n: u32, at: f64, s: &mut Script| -> bool {
        if s.done < n && t >= at {
            s.done = n;
            true
        } else {
            false
        }
    };
    let eye = **player;
    let fwd = Vec3::new(eye.forward().x, 0.0, eye.forward().z).normalize_or(Vec3::NEG_Z);
    let right = fwd.cross(Vec3::Y);
    let feet = eye.translation - Vec3::Y * 1.5;
    if step(1, 1.0, &mut script) {
        pen.equipped = true;
    }
    if mode == "build" {
        if step(2, 1.5, &mut script) {
            let row: [(Kind, Vec3); 9] = [
                (Kind::Shape { shape: Shape::Cube, color: 0, material: 9 }, Vec3::splat(0.5)),
                (Kind::Shape { shape: Shape::Cube, color: 1, material: 15 }, Vec3::splat(0.5)),
                (Kind::Shape { shape: Shape::Sphere, color: 2, material: 15 }, Vec3::splat(0.5)),
                (Kind::Shape { shape: Shape::Cylinder, color: 3, material: 6 }, Vec3::splat(0.5)),
                (Kind::Shape { shape: Shape::Wedge, color: 13, material: 12 }, Vec3::new(1.0, 0.75, 1.5)),
                (Kind::Shape { shape: Shape::Cone, color: 45, material: 1 }, Vec3::splat(0.5)),
                (Kind::Shape { shape: Shape::Torus, color: 28, material: 15 }, Vec3::splat(0.6)),
                (Kind::Prop(PROPS[7].address.into()), Vec3::ONE),
                (Kind::Prop(PROPS[1].address.into()), Vec3::ONE),
            ];
            for (i, (kind, scale)) in row.into_iter().enumerate() {
                let x = (i as f32 - 4.0) * 0.9;
                let lift = if matches!(kind, Kind::Prop(_)) { 0.0 } else { scale.y * 0.5 };
                let at = Transform { translation: feet + fwd * 3.0 + right * x + Vec3::Y * lift, rotation: Quat::from_rotation_y((-fwd.x).atan2(-fwd.z)), scale };
                let uid = super::new_uid();
                let s = ObjState { address: kind.address(uid), at };
                pen.history.push(Change { uid, before: None, after: Some(s.clone()) });
                pen.queue.push_back(s);
            }
        }
        if step(3, 2.0, &mut script) {
            pen.request = Some(Request::Builders);
        }
        if step(4, 4.0, &mut script) {
            if let Some((e, m)) = made.iter().filter(|m| matches!(m.1.kind, Kind::Shape { shape: Shape::Cube, color: 0, .. })).map(|m| (m.0, m.1.clone())).next() {
                if net.request_ownership(e) {
                    net.send_rpc(e, "MkSel", vec![1], RpcTarget::Others);
                    let before = made.get(e).ok().map(|x| ObjState { address: m.kind.address(m.uid), at: *x.3 });
                    pen.sel = Some(Sel { entity: e, uid: m.uid, dist: 3.0, before, kind: m.kind, look_changed: false, fresh: 0 });
                }
            }
        }
        if (4.0..6.0).contains(&t) {
            if let Some(sel) = pen.sel.as_mut() {
                if let Ok(mut x) = made.get_mut(sel.entity) {
                    x.3.translation = feet + fwd * 2.5 + Vec3::Y * (0.5 + (t - 4.0) as f32 * 0.6);
                    x.3.rotation = Quat::from_rotation_y((t - 4.0) as f32 * 0.8);
                    if t > 5.0 && !sel.look_changed {
                        if let Kind::Shape { color, .. } = &mut sel.kind {
                            *color = 7;
                        }
                        x.3.scale = Vec3::splat(0.8);
                        sel.look_changed = true;
                    }
                }
            }
        }
        if step(5, 6.0, &mut script) {
            put_down(pen, &mut net, &made, now);
        }
        if step(6, 8.0, &mut script) {
            pen.request = Some(Request::Save);
        }
    } else if mode == "move" {
        if t >= 7.0 && script.done < 2 && pen.sel.is_none() {
            if !pen.can_edit() {
                // ask again now and then: the owner may have let us in
                if (t * 2.0).fract() < 0.02 {
                    save::run(|l| save::Answer::Info(save::fetch(l)));
                }
                return;
            }
            let near = made.iter().min_by(|a, b| a.3.translation.distance(eye.translation).total_cmp(&b.3.translation.distance(eye.translation)))
                .map(|m| (m.0, m.1.clone(), *m.3));
            if let Some((e, m, at)) = near {
                if net.request_ownership(e) {
                    net.send_rpc(e, "MkSel", vec![1], RpcTarget::Others);
                    let before = Some(ObjState { address: m.kind.address(m.uid), at });
                    pen.sel = Some(Sel { entity: e, uid: m.uid, dist: 2.0, before, kind: m.kind, look_changed: false, fresh: 0 });
                    script.done = 2;
                    script.start = Some(now - 7.0);
                }
            }
        }
        if script.done == 2 && (7.0..11.0).contains(&t) {
            if let Some(sel) = pen.sel.as_ref() {
                if let Ok(mut x) = made.get_mut(sel.entity) {
                    let a = (t - 7.0) as f32 * 1.5;
                    x.3.translation = feet + fwd * 2.0 + right * a.sin() * 0.8 + Vec3::Y * (1.2 + 0.3 * a.cos());
                }
            }
        }
        if script.done == 2 && t >= 11.0 {
            script.done = 3;
            put_down(pen, &mut net, &made, now);
        }
        if step(4, 12.0, &mut script) {
            let near = made.iter().min_by(|a, b| a.3.translation.distance(eye.translation).total_cmp(&b.3.translation.distance(eye.translation)))
                .map(|m| (m.0, m.1.clone(), *m.3));
            if let Some((e, m, at)) = near {
                pen.history.push(Change { uid: m.uid, before: Some(ObjState { address: m.kind.address(m.uid), at }), after: None });
                net.destroy(e);
            }
        }
        let _ = hold_new;
    }
}
