// Ingen konsollvindu i ferdig Windows-bygg.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cloudjumper_lib::run()
}
