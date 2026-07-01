// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--playback-daemon") {
        let _instance = wave_lib::single_instance::try_acquire(
            wave_lib::single_instance::InstanceMode::Daemon,
        )
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        });
        wave_lib::playback_daemon::run_daemon();
        return;
    }

    if args.len() > 1 {
        if wave_lib::cli::is_daemon_ipc_client(&args) {
            wave_lib::cli::run();
            return;
        }
        if wave_lib::single_instance::primary_is_running() {
            eprintln!("{}", wave_lib::single_instance::already_running_message());
            std::process::exit(1);
        }
        wave_lib::cli::run();
    } else {
        wave_lib::run();
    }
}
