// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--probe` prints what the headset reports and changes nothing.
    // `--probe-write` also re-sends the noise mode it is already in.
    let has = |flag: &str| std::env::args().any(|arg| arg == flag);
    if has("--probe") || has("--probe-write") {
        open_headphones_connect_lib::bluetooth::probe::run(has("--probe-write"));
        return;
    }
    open_headphones_connect_lib::run()
}
