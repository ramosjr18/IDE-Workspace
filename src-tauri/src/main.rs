// Evita que en Windows se abra una consola junto a la app en la versión final.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `agentes run …` dentro de la app: el mismo ejecutable hace de cliente y sale.
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--agentes-cli") {
        std::process::exit(ide_workspace_chat_lib::cli_main(args.collect()));
    }
    ide_workspace_chat_lib::run()
}
