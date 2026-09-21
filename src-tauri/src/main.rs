// 发布版隐藏控制台窗口。不加这行的话，双击 exe 会额外弹出一个黑色命令行窗口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    glassnote_lib::run()
}
