mod app;
mod ui;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("shadow-convert {}", shadow_convert::paths::APP_VERSION);
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--detect-hw") {
        let caps = shadow_convert::hw::detect_now();
        println!("ffmpeg: {}", caps.ffmpeg_found);
        println!("encoders: {}", caps.summary());
        println!("nvenc_h264: {}", caps.nvenc_h264);
        println!("nvenc_hevc: {}", caps.nvenc_hevc);
        println!("nvenc_av1: {}", caps.nvenc_av1);
        println!("libx264: {}", caps.software_x264);
        return ExitCode::SUCCESS;
    }
    app::run()
}

fn print_help() {
    println!(
        "Shadow Convert — convert video, audio, and images locally\n\n\
         Usage:\n\
         \tshadow-convert [OPTIONS] [FILE...]\n\n\
         Options:\n\
         \t--detect-hw     Print detected hardware and software encoders\n\
         \t--help, -h      Show this help\n\
         \t--version, -V   Show version\n"
    );
}
