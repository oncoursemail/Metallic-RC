//! Opens a window showing one menu page from the installed game, as the client's menu builds it, drawn with Bevy UI.
//! `cargo run --release -p metallic-rc-client --example ui_view [address] [game dir]`
use std::collections::HashMap;

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::PrimaryWindow;
use metallic_rc_client::room;
use metallic_rc_client::settings::Settings;
use metallic_rc_client::ui::{menu, view};

#[derive(Resource)]
struct Shown(menu::Painter);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let address = args.first().cloned().unwrap_or_else(|| menu::ESCAPE.into());
    let game = args.get(1).map(std::path::PathBuf::from)
        .or_else(|| room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()))
        .expect("no game folder");
    let painter = menu::Painter::start(room::bundle_dir(&game));
    painter.ask(menu::Request { page: address.clone(), back_stack: 1, settings: Settings::default(), account: "Player".into(), typed: String::new(),
        focused: false, scroll: HashMap::new(), choosing: None, server: String::new(), token: String::new() });
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: format!("Metallic RC: {address}"), ..default() }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.18, 0.2, 0.24)))
        .insert_resource(Shown(painter))
        .init_resource::<view::Uploaded>()
        .add_systems(Startup, |mut c: Commands| { c.spawn(Camera2d); })
        .add_systems(Update, (show, shot))
        .run();
}

/// With METALLIC_SHOT=<path> set, screenshot after it has laid out, then exit.
fn shot(mut commands: Commands, time: Res<Time>, mut done: Local<bool>, mut exit: MessageWriter<AppExit>) {
    let Ok(path) = std::env::var("METALLIC_SHOT") else { return };
    let at: f32 = std::env::var("METALLIC_SHOT_AT").ok().and_then(|s| s.parse().ok()).unwrap_or(6.0);
    let t = time.elapsed_secs();
    if !*done && t > at {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(std::path::PathBuf::from(path)));
        *done = true;
    }
    if *done && t > at + 1.5 {
        exit.write(AppExit::Success);
    }
}

fn show(mut commands: Commands, shown: Res<Shown>, window: Single<&Window, With<PrimaryWindow>>, mut images: ResMut<Assets<Image>>, mut fonts: ResMut<Assets<Font>>,
    mut uploaded: ResMut<view::Uploaded>) {
    match shown.0.take() {
        Some(Ok(p)) => drop(view::spawn(&mut commands, &mut images, &mut fonts, &mut uploaded, &p.drawn, window.size())),
        Some(Err(e)) => eprintln!("{e}"),
        None => {}
    }
}
