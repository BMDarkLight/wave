// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Detect CLI mode: if any arguments are present beyond the binary name,
    // or if --cli/--headless is passed, run in CLI mode.
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        wave_lib::cli::run();
    } else {
        wave_lib::run()
    }
}
