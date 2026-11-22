//! kemacs Rust 版のエントリポイント。

use std::io::{self, Write};

use kemacs::buffer::{MDBINARY, MDVIEW};
use kemacs::editor::Editor;
use kemacs::encoding::{Code, Eol};
use kemacs::input::{self, TerminalIo};
use kemacs::script;

fn print_usage(prog: &str) {
    eprintln!(
        "Usage: {} [-h|--help] [--code UTF8|UJIS|SJIS|JIS] [options] [file ...]",
        prog
    );
    eprintln!("  -v, -V          VIEW モード (読み取り専用)");
    eprintln!("  -e, -E          EDIT モード (既定)");
    eprintln!("  -b              BINARY モード (フラグのみ。変換は停止しない)");
    eprintln!("  -g<line>        起動時に指定行へ移動");
    eprintln!("  -x<command>     起動時にコマンドを実行");
    eprintln!("  -f<code>        新規ファイルの文字コード (-fUTF8 等)");
    eprintln!("  -t<code>        端末文字コード (-tUTF8 等。$bcode の別名)");
    eprintln!("  -r, -R          制限モード (フラグのみ)");
    eprintln!("  @file           スタートアップマクロを実行");
    eprintln!("  未移植: -s<pattern>, -k<key>");
    eprintln!("  Rust port of kemacs (core command set).");
}

/// マクロファイルを 1 行ずつ docmd で実行する (C 版 startup() 相当)。
fn run_macro_file(ed: &mut Editor, path: &str) {
    let Ok(data) = std::fs::read(path) else {
        return;
    };
    let text = String::from_utf8_lossy(&data);
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let _ = script::docmd(ed, line);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage("kemacs");
        return;
    }

    let (rows, cols) = input::terminal_size();
    let mut ed = Editor::new(Box::new(TerminalIo::new()), rows, cols);

    let mut has_file = false;
    let mut forced_code: Option<(Code, Eol)> = None;
    let mut files: Vec<String> = Vec::new();
    let mut view_mode = false;
    let mut binary_mode = false;
    let mut goto_line: Option<usize> = None;
    let mut startup_commands: Vec<String> = Vec::new();
    let mut macro_files: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let arg = &args[i];
        if (arg == "--code" || arg == "-u") && i + 1 < args.len() {
            let (c, e) = Code::parse(&args[i + 1]);
            ed.default_code = c;
            ed.default_eol = e;
            forced_code = Some((c, e));
            i += 2;
            continue;
        }
        if let Some(v) = arg
            .strip_prefix("--code=")
            .or_else(|| arg.strip_prefix("-u"))
        {
            let (c, e) = Code::parse(v);
            ed.default_code = c;
            ed.default_eol = e;
            forced_code = Some((c, e));
            i += 1;
            continue;
        }
        if arg == "-v" || arg == "-V" {
            view_mode = true;
            i += 1;
            continue;
        }
        if arg == "-e" || arg == "-E" {
            view_mode = false;
            i += 1;
            continue;
        }
        if arg == "-b" {
            binary_mode = true;
            i += 1;
            continue;
        }
        if arg == "-r" || arg == "-R" {
            // 制限モードはフラグのみ (変換・コマンド制限は未実装)
            i += 1;
            continue;
        }
        if let Some(v) = arg.strip_prefix("-g") {
            let v = if v.is_empty() && i + 1 < args.len() {
                i += 1;
                &args[i]
            } else {
                v
            };
            if let Ok(n) = v.trim().parse::<usize>() {
                goto_line = Some(n);
            }
            i += 1;
            continue;
        }
        if let Some(v) = arg.strip_prefix("-x") {
            let v = if v.is_empty() && i + 1 < args.len() {
                i += 1;
                &args[i]
            } else {
                v
            };
            startup_commands.push(v.to_string());
            i += 1;
            continue;
        }
        if let Some(v) = arg.strip_prefix("-f").or_else(|| arg.strip_prefix("-t")) {
            let (c, e) = Code::parse(v);
            ed.default_code = c;
            ed.default_eol = e;
            i += 1;
            continue;
        }
        if let Some(path) = arg.strip_prefix('@') {
            macro_files.push(path.to_string());
            i += 1;
            continue;
        }
        if arg.starts_with('-') {
            i += 1;
            continue;
        }
        files.push(arg.clone());
        i += 1;
    }
    for arg in &files {
        let path = std::path::Path::new(arg);
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| arg.clone());
        // 新規ファイルでもファイル名はバッファに設定する (C 版の動作)
        let data = std::fs::read(path).unwrap_or_default();
        let (code, eol) = if data.is_empty() {
            forced_code.unwrap_or((ed.default_code, ed.default_eol))
        } else {
            forced_code.unwrap_or((Code::detect(&data), Eol::detect(&data)))
        };
        let bi = if !has_file {
            has_file = true;
            0
        } else {
            ed.find_buffer_or_create(&name)
        };
        {
            let b = &mut ed.buffers[bi];
            b.name = name.clone();
            b.fname = Some(arg.clone());
            b.load_encoded(&data, code, eol);
        }
        kemacs::modes::auto_mode(&mut ed, bi, &name);
    }
    if !has_file {
        ed.buffers[0].name = "main".to_string();
    }
    ed.window_mut().buf = 0;

    for b in &mut ed.buffers {
        if view_mode {
            b.mode |= MDVIEW;
        }
        if binary_mode {
            b.mode |= MDBINARY;
        }
    }

    // C 版の起動ファイル (~/.kemacsrc) を自動実行する。
    if let Ok(home) = std::env::var("HOME") {
        let path = std::path::Path::new(&home).join(".kemacsrc");
        if path.exists() {
            run_macro_file(&mut ed, &path.to_string_lossy());
        }
    }
    for path in &macro_files {
        run_macro_file(&mut ed, path);
    }
    for cmd in &startup_commands {
        let _ = script::docmd(&mut ed, cmd);
    }
    if let Some(n) = goto_line {
        ed.buffer_mut().goto_line(n);
    }

    if let Err(e) = input::enable() {
        eprintln!("failed to enable raw mode: {}", e);
        std::process::exit(1);
    }
    let result = input::run(&mut ed);
    let _ = input::disable();
    let _ = io::stdout().flush();
    if let Err(e) = result {
        eprintln!("{}", e);
    }
}
