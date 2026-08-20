#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if let Some(index) = args.iter().position(|value| value == "--command") {
        if let Some(command) = args.get(index + 1) {
            let handled = qizhuo_lib::send_command(command);
            if handled || command == "quit" {
                return;
            }
        }
    }
    qizhuo_lib::run();
}
