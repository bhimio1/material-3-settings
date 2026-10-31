//! Material 3 Settings — a settings manager for Arch Linux, themed with
//! Material You to match the Material 3 File Manager.

mod app;
mod catalog;
mod config;
mod end4;
mod files;
mod formats;
mod hypr;
mod inject;
mod keybinds;
mod login;
mod material_apps;
mod sys;
mod ui;
mod welcome;

const USAGE: &str = "Usage: m3-settings [OPTIONS] [PAGE]

PAGE opens a page directly: overview, appearance, display, sound, network,
bluetooth, power, datetime, default-apps, packages, about, hyprland, end4,
shells, login, apps, preferences, or the id of an app on the Apps page
(kitty, waybar, …).

  material-apps [APP]   the Material 3 apps page, or one app's page, by
                        name or binary (music, m3-music, calendar, …)
  welcome               same as --welcome
  inject [CMD] [SHELL]  add the Material 3 integration to a stock shell
                        (ii, end4-pC, inir, dux) without opening a window.
                        CMD is status (default), apply, remove, or reapply
                        (every shell it was added to whose update replaced
                        it; the systemd path unit runs this)
  login [CMD]           the login screen and boot animation: status
                        (default), sync (copy the shell's look to the
                        login screen; a systemd path unit runs this),
                        install-sddm, remove-sddm, install-plymouth
  keybinds [CMD]        the Material 3 apps' Hyprland keybinds: status
                        (default), add (only keys that are still free),
                        remove

Options:
  -w, --welcome   Open the first-run welcome: wallpaper and colors,
                  Material 3 apps, default apps and keyboard shortcuts.
                  The shell integration runs this at first login.
  -h, --help      Show this help
  -V, --version";

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let first = std::env::args().nth(1);
    if first.as_deref() == Some("inject") {
        std::process::exit(inject_cli(std::env::args().skip(2).collect()));
    }
    if first.as_deref() == Some("login") {
        std::process::exit(login_cli(std::env::args().nth(2)));
    }
    if first.as_deref() == Some("keybinds") {
        std::process::exit(keybinds_cli(std::env::args().nth(2)));
    }

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

fn report(r: Result<String, String>) -> i32 {
    match r {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// `m3-settings login …`. Returns the exit code.
fn login_cli(cmd: Option<String>) -> i32 {
    let home = material_3_ui::sys::paths::home();
    match cmd.as_deref().unwrap_or("status") {
        "status" => {
            let s = login::status();
            let yes = |b: bool| if b { "yes" } else { "no" };
            println!("login screen   installed: {}, selected: {} (SDDM shows {})", yes(s.sddm_installed), yes(s.sddm_selected()), s.sddm_current.as_deref().unwrap_or("its default"));
            if !s.sddm_available {
                println!("               SDDM isn't installed");
            }
            if let Some(dm) = &s.other_dm {
                println!("               the login manager is {dm}, not SDDM");
            }
            println!("boot animation installed: {}, selected: {}", yes(s.plymouth_installed), yes(s.plymouth_selected()));
            if !s.plymouth_available {
                println!("               Plymouth isn't installed");
            }
            0
        }
        "sync" => report(login::sync()),
        "install-sddm" => report(login::install_sddm(&home)),
        "remove-sddm" => report(login::remove_sddm(&home)),
        "install-plymouth" => report(login::install_plymouth()),
        other => {
            eprintln!("Unknown login command: {other} (status, sync, install-sddm, remove-sddm or install-plymouth)");
            2
        }
    }
}

/// `m3-settings keybinds …`. Returns the exit code.
fn keybinds_cli(cmd: Option<String>) -> i32 {
    let home = material_3_ui::sys::paths::home();
    let backups = config::Config::load().backups;
    match cmd.as_deref().unwrap_or("status") {
        "status" => {
            let p = keybinds::plan(&home, &keybinds::live_keys(), keybinds::have_games(&home));
            println!("{} (added: {})", keybinds::target(&home).0.display(), p.added);
            for (s, free) in p.binds {
                println!("  {:<18} {:<34} {}", s.key, s.description, if free { "free" } else { "taken, left alone" });
            }
            0
        }
        "add" | "apply" => report(keybinds::apply(&home, backups)),
        "remove" => report(keybinds::remove(&home, backups)),
        other => {
            eprintln!("Unknown keybinds command: {other} (status, add or remove)");
            2
        }
    }
}

/// `m3-settings inject …`. Returns the exit code.
fn inject_cli(args: Vec<String>) -> i32 {
    use inject::{State, Target};
    let home = material_3_ui::sys::paths::home();
    let cmd = args.first().map(String::as_str).unwrap_or("status");
    let targets: Vec<Target> = match args.get(1) {
        Some(name) => match Target::parse(name) {
            Some(t) => vec![t],
            None => {
                eprintln!("Unknown shell: {name} (ii, end4-pC, inir or dux)");
                return 2;
            }
        },
        None => Target::ALL.to_vec(),
    };
    let mut code = 0;
    match cmd {
        "status" => {
            for t in targets {
                let s = inject::status(&home, t);
                let state = match s.state {
                    State::NotInstalled => "not installed".to_string(),
                    State::Clean => "installed, not added".to_string(),
                    State::Applied { pack_version } => format!("added (pack {pack_version})"),
                    State::NeedsReapply => "needs adding again".to_string(),
                };
                println!("{:<8} {state}", t.id());
                for sk in s.record.iter().flat_map(|r| &r.skipped) {
                    println!("         left out {}: {}", sk.feature, sk.reason);
                }
            }
        }
        "apply" | "add" => {
            for t in targets.into_iter().filter(|t| t.is_installed(&home)) {
                match ui::pages::shells::apply_one(t) {
                    Ok(msg) => println!("{msg}"),
                    Err(e) => {
                        eprintln!("{}: {e}", t.id());
                        code = 1;
                    }
                }
            }
        }
        "remove" => {
            for t in targets {
                if !matches!(inject::status(&home, t).state, State::Applied { .. } | State::NeedsReapply) {
                    continue;
                }
                match ui::pages::shells::remove_one(t) {
                    Ok(msg) => println!("{msg}"),
                    Err(e) => {
                        eprintln!("{}: {e}", t.id());
                        code = 1;
                    }
                }
            }
        }
        "reapply" => {
            for (t, r) in inject::reapply_all(&home) {
                match r {
                    Ok(rep) => println!("{}", rep.summary(t)),
                    Err(e) => {
                        eprintln!("{}: {e}", t.id());
                        code = 1;
                    }
                }
            }
        }
        other => {
            eprintln!("Unknown inject command: {other} (status, apply, remove or reapply)");
            return 2;
        }
    }
    code
}
