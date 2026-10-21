//! Material 3 Settings — a settings manager for Arch Linux, themed with
//! Material You to match the Material 3 File Manager.

mod app;
mod catalog;
mod config;
mod end4;
mod files;
mod formats;
mod hypr;
mod material_apps;
mod sys;
mod ui;
mod welcome;

const USAGE: &str = "Usage: m3-settings [OPTIONS] [PAGE]

PAGE opens a page directly: overview, appearance, display, sound, network,
bluetooth, power, datetime, default-apps, packages, about, hyprland, end4,
apps, preferences, or the id of an app on the Apps page (kitty, waybar, …).

  material-apps [APP]   the Material 3 apps page, or one app's page, by
                        name or binary (music, m3-music, calendar, …)
  welcome               same as --welcome

Options:
  -w, --welcome   Open the first-run welcome: wallpaper and colors,
                  Material 3 apps, default apps and keyboard shortcuts.
                  The end4-pC shell runs this at first login.
  -h, --help      Show this help
  -V, --version";

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let mut start = None;
    let mut welcome = false;
    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("m3-settings {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-w" | "--welcome" | "welcome" => welcome = true,
            other if other.starts_with('-') => {
                eprintln!("Unknown option: {other}\n\n{USAGE}");
                std::process::exit(2);
            }
            "material-apps" | "m3-apps" => {
                start = Some(match args.next_if(|a| !a.starts_with('-')) {
                    Some(key) => match material_apps::find(&key) {
                        Some(a) => ui::Page::MaterialApp(a.id.to_string()),
                        None => {
                            eprintln!("Unknown Material 3 app: {key}\n\n{USAGE}");
                            std::process::exit(2);
                        }
                    },
                    None => ui::Page::MaterialApps,
                });
            }
            page => match ui::Page::parse(page) {
                Some(p) => start = Some(p),
                None => {
                    eprintln!("Unknown page: {page}\n\n{USAGE}");
                    std::process::exit(2);
                }
            },
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Settings")
            .with_app_id("material_3_settings")
            .with_inner_size([1180.0, 800.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "material_3_settings",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, start, welcome)))),
    )
}
