#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg(feature = "tauri")]

fn main() {
    synapse_kb_lib::run();
}
