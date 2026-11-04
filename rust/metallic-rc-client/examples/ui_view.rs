//! Opens a window showing one menu page from the installed game, drawn with Bevy UI.
//! `cargo run --release -p metallic-rc-client --example ui_view [address] [width height] [game dir]`
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use metallic_rc_client::room::{self, Library};
use metallic_rc_client::ui::{paint, prefab, theme, view};

#[derive(Resource)]
struct Shown(Option<paint::Drawn>);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let address = args.first().cloned().unwrap_or_else(|| "(EscapeMenu_standalone)".into());
    let canvas = match (args.get(1).and_then(|a| a.parse().ok()), args.get(2).and_then(|a| a.parse().ok())) {
        (Some(w), Some(h)) => [w, h],
        _ => [1000.0, 1150.0],
    };
    let game = args.get(3).map(std::path::PathBuf::from)
        .or_else(|| room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists()))
        .expect("no game folder");
    let dir = room::bundle_dir(&game);
    let pages = prefab::Pages::open(&dir).unwrap();
    let mut lib = Library::new(&dir).unwrap();
    let mut page = prefab::load(&mut lib, &pages, &dir, &address).unwrap();
    theme::Theme::load(&mut lib, &pages, &dir).unwrap().apply(&mut page);
    let drawn = paint::page(&mut lib, &page, canvas);
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: format!("Metallic RC: {address}"), ..default() }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.18, 0.2, 0.24)))
        .insert_resource(Shown(Some(drawn)))
        .add_systems(Startup, |mut c: Commands| { c.spawn(Camera2d); })
        .add_systems(Update, show)
        .run();
}

fn show(mut commands: Commands, mut shown: ResMut<Shown>, window: Single<&Window, With<PrimaryWindow>>, mut images: ResMut<Assets<Image>>, mut fonts: ResMut<Assets<Font>>) {
    if let Some(d) = shown.0.take() {
        view::spawn(&mut commands, &mut images, &mut fonts, &d, window.size());
    }
}
