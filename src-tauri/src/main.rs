// Evita que en Windows se abra una consola junto a la app en la versión final.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ide_workspace_chat_lib::run()
}
