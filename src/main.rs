mod board;
mod display;
mod verilog;

use board::Board;
use display::{LedWidget, Seg7Widget};
use verilog::lexer::Lexer;
use verilog::parser::Parser;
use verilog::simulator::Simulator;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;
use std::collections::HashSet;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

const TICK_RATE_MS: u64 = 16; // ~60fps
const DEFAULT_CYCLES_PER_FRAME: u64 = 1_000; //  833_333; // 50MHz

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <file.v> [file.v ...] [--top <module>] [--speed <cycles_per_frame>]", args[0]);
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  {} src_ans/day1/seg7dec.v", args[0]);
        eprintln!("  {} src_ans/day3/sec60_for_ModelSim.v src_ans/day3/seg7dec_u.v", args[0]);
        eprintln!("  {} src_ans/day1/sec10.v --speed 50000", args[0]);
        std::process::exit(1);
    }

    // Parse CLI args
    let mut files = Vec::new();
    let mut top_name: Option<String> = None;
    let mut cycles_per_frame = DEFAULT_CYCLES_PER_FRAME;
    let mut bench = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--top" => {
                i += 1;
                top_name = Some(args[i].clone());
            }
            "--speed" => {
                i += 1;
                cycles_per_frame = args[i].parse().unwrap_or(DEFAULT_CYCLES_PER_FRAME);
            }
            "--bench" => bench = true,
            _ => files.push(args[i].clone()),
        }
        i += 1;
    }

    // Parse all Verilog files
    let mut all_modules = Vec::new();
    let mut loaded_files: HashSet<String> = HashSet::new();
    for file in &files {
        parse_file_into(file, &mut all_modules, &mut loaded_files);
    }

    // Build simulator, auto-discovering missing module dependencies
    let search_dirs = collect_search_dirs(&files);
    let mut sim = loop {
        match Simulator::build(&all_modules, top_name.as_deref()) {
            Ok(s) => break s,
            Err(e) => {
                // Try to auto-discover missing module
                if let Some(missing) = e.strip_prefix("Module '").and_then(|s| s.split('\'').next()) {
                    if let Some(path) = find_module_file(missing, &search_dirs, &loaded_files) {
                        eprintln!("Auto-discovered dependency: {} -> {}", missing, path);
                        parse_file_into(&path, &mut all_modules, &mut loaded_files);
                        continue;
                    }
                }
                eprintln!("Compilation error: {}", e);
                std::process::exit(1);
            }
        }
    };

    eprintln!("Loaded module: {} ({} cycles/frame)", sim.top_name, cycles_per_frame);

    if bench {
        // Headless benchmark: run 60 frames and report timing
        let mut board = Board::new();
        sim.read_inputs(&board);
        sim.settle();
        let start = Instant::now();
        let frames = 60;
        for _ in 0..frames {
            for _ in 0..cycles_per_frame {
                sim.tick();
            }
        }
        let elapsed = start.elapsed();
        sim.write_outputs(&mut board);
        let total_cycles = frames * cycles_per_frame;
        eprintln!(
            "Benchmark: {} cycles in {:.3}s ({:.1}M cycles/sec, {:.1}ms/frame)",
            total_cycles,
            elapsed.as_secs_f64(),
            total_cycles as f64 / elapsed.as_secs_f64() / 1_000_000.0,
            elapsed.as_secs_f64() / frames as f64 * 1000.0,
        );
        // Print board state
        for i in 0..6 {
            let mut val = 0u8;
            for bit in 0..7 {
                if board.hex[i][bit] { val |= 1 << bit; }
            }
            if val != 0x7F { // not all-off
                eprint!("HEX{}={:07b} ", i, val);
            }
        }
        let led_val: u16 = board.ledr.iter().enumerate()
            .fold(0u16, |acc, (i, &on)| if on { acc | (1 << i) } else { acc });
        if led_val != 0 {
            eprint!("LEDR={:010b}", led_val);
        }
        eprintln!();
        return Ok(());
    }

    // Setup terminal
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, sim, cycles_per_frame, &files);

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn parse_file_into(
    file: &str,
    modules: &mut Vec<verilog::ast::VerilogModule>,
    loaded: &mut HashSet<String>,
) {
    let canonical = std::fs::canonicalize(file)
        .unwrap_or_else(|_| file.into())
        .to_string_lossy()
        .to_string();
    if !loaded.insert(canonical) {
        return; // already loaded
    }
    let source = std::fs::read_to_string(file).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", file, e);
        std::process::exit(1);
    });
    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    match parser.parse_file() {
        Ok(m) => modules.extend(m),
        Err(e) => {
            eprintln!("Parse error in {}: {}", file, e);
            std::process::exit(1);
        }
    }
}

fn collect_search_dirs(files: &[String]) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for f in files {
        if let Some(parent) = Path::new(f).parent() {
            let dir = parent.to_string_lossy().to_string();
            if !dirs.contains(&dir) {
                dirs.push(dir.clone());
            }
            // Also add sibling directories (e.g., day1, day2, day3 under src_ans)
            if let Some(grandparent) = parent.parent() {
                if let Ok(entries) = std::fs::read_dir(grandparent) {
                    for entry in entries.flatten() {
                        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            let sibling = entry.path().to_string_lossy().to_string();
                            if !dirs.contains(&sibling) {
                                dirs.push(sibling);
                            }
                        }
                    }
                }
            }
        }
    }
    dirs
}

fn find_module_file(
    module_name: &str,
    search_dirs: &[String],
    loaded: &HashSet<String>,
) -> Option<String> {
    let lower = module_name.to_lowercase();
    for dir in search_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "v") {
                    let canon = std::fs::canonicalize(&path)
                        .unwrap_or_else(|_| path.clone())
                        .to_string_lossy()
                        .to_string();
                    if loaded.contains(&canon) {
                        continue;
                    }
                    // Check if this file likely contains the module
                    // by reading it and looking for the module declaration
                    if let Ok(source) = std::fs::read_to_string(&path) {
                        let file_lower = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        // Quick check: filename matches or file contains module declaration
                        if file_lower == lower
                            || source.contains(&format!("module {}", module_name))
                            || source.contains(&format!("module  {}", module_name))
                        {
                            return Some(path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut sim: Simulator,
    cycles_per_frame: u64,
    files: &[String],
) -> io::Result<()> {
    let mut board = Board::new();
    let mut total_cycles: u64 = 0;
    let mut paused = false;

    // Initial combinational settle
    sim.read_inputs(&board);
    sim.settle();
    sim.write_outputs(&mut board);

    loop {
        let tick_start = Instant::now();

        // Handle input
        while event::poll(Duration::ZERO)? {
            if let Event::Key(key_event) = event::read()? {
                if key_event.kind != KeyEventKind::Press {
                    continue;
                }
                match key_event.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char(' ') => paused = !paused,
                    KeyCode::Char('r') => board.rst = true,

                    // KEY[0-3] push buttons
                    KeyCode::Char('1') => board.key[0] = true,
                    KeyCode::Char('2') => board.key[1] = true,
                    KeyCode::Char('3') => board.key[2] = true,
                    KeyCode::Char('4') => board.key[3] = true,

                    // SW[0-9] toggle switches
                    KeyCode::F(n) if (1..=10).contains(&n) => {
                        let idx = (n - 1) as usize;
                        if idx < 10 {
                            board.sw[idx] = !board.sw[idx];
                        }
                    }

                    _ => {}
                }
            }
        }

        if !paused {
            // Simulate as many cycles as possible within frame budget
            sim.read_inputs(&board);
            let sim_start = Instant::now();
            let budget = Duration::from_millis(TICK_RATE_MS - 2); // leave 2ms for rendering
            let mut frame_cycles = 0u64;
            while sim_start.elapsed() < budget && frame_cycles < cycles_per_frame {
                // Run in batches to reduce time-check overhead
                let batch = 8192.min(cycles_per_frame - frame_cycles);
                for _ in 0..batch {
                    sim.tick();
                }
                frame_cycles += batch;
            }
            sim.write_outputs(&mut board);
            total_cycles += frame_cycles;
        }

        // Clear one-shot signals
        board.rst = false;
        for k in &mut board.key {
            *k = false;
        }

        // Render
        let top_name = sim.top_name.clone();
        terminal.draw(|frame| {
            let size = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),  // Title
                    Constraint::Length(8),  // 7-seg
                    Constraint::Length(4),  // LEDs
                    Constraint::Length(5),  // Switches
                    Constraint::Min(3),    // Help
                ])
                .split(size);

            // Title
            let status = if paused { "PAUSED" } else { "RUNNING" };
            let title = Paragraph::new(format!(
                " Module: {}  |  Cycles: {}  |  {}",
                top_name, total_cycles, status
            ))
            .style(Style::default().fg(Color::Cyan))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" DE0-CV Emulator "),
            );
            frame.render_widget(title, chunks[0]);

            // 7-segment displays
            let hex_block = Block::default()
                .borders(Borders::ALL)
                .title(" 7-Segment Displays (HEX5..HEX0) ");
            let hex_inner = hex_block.inner(chunks[1]);
            frame.render_widget(hex_block, chunks[1]);

            for i in 0..6 {
                let display_idx = 5 - i;
                let x = hex_inner.x + (i as u16) * 7 + 1;
                if x + 5 <= hex_inner.x + hex_inner.width {
                    let seg = Seg7Widget::new(board.hex[display_idx]);
                    frame.render_widget(seg, Rect::new(x, hex_inner.y, 5, 5));
                    let label = Paragraph::new(format!("HEX{}", display_idx))
                        .style(Style::default().fg(Color::DarkGray));
                    frame.render_widget(label, Rect::new(x, hex_inner.y + 5, 5, 1));
                }
            }

            // LEDs
            let led_block = Block::default()
                .borders(Borders::ALL)
                .title(" LEDs (LEDR[9..0]) ");
            let led_inner = led_block.inner(chunks[2]);
            frame.render_widget(led_block, chunks[2]);

            for i in 0..10 {
                let led_idx = 9 - i;
                let x = led_inner.x + (i as u16) * 4 + 1;
                if x + 2 <= led_inner.x + led_inner.width {
                    let led = LedWidget::new(board.ledr[led_idx], format!("{}", led_idx));
                    frame.render_widget(led, Rect::new(x, led_inner.y, 3, 2));
                }
            }

            // Switches
            let sw_block = Block::default()
                .borders(Borders::ALL)
                .title(" Switches - Toggle: F1-F10 ");
            let sw_inner = sw_block.inner(chunks[3]);
            frame.render_widget(sw_block, chunks[3]);

            for i in 0..10 {
                let sw_idx = 9 - i;
                let x = sw_inner.x + (i as u16) * 5 + 1;
                if x + 4 <= sw_inner.x + sw_inner.width {
                    let on = board.sw[sw_idx];
                    let (symbol, style) = if on {
                        ("ON", Style::default().fg(Color::Green))
                    } else {
                        ("OF", Style::default().fg(Color::DarkGray))
                    };
                    frame.render_widget(
                        Paragraph::new(symbol).style(style),
                        Rect::new(x, sw_inner.y, 4, 1),
                    );
                    let key_label = if sw_idx < 9 {
                        format!("F{}", sw_idx + 1)
                    } else {
                        "F10".to_string()
                    };
                    frame.render_widget(
                        Paragraph::new(key_label).style(Style::default().fg(Color::White)),
                        Rect::new(x, sw_inner.y + 1, 4, 1),
                    );
                    frame.render_widget(
                        Paragraph::new(format!("SW{}", sw_idx))
                            .style(Style::default().fg(Color::DarkGray)),
                        Rect::new(x, sw_inner.y + 2, 4, 1),
                    );
                }
            }

            // Help
            let file_list: String = files.iter().map(|f| {
                f.rsplit('/').next().unwrap_or(f)
            }).collect::<Vec<_>>().join(", ");

            let help = Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("Files: ", Style::default().fg(Color::Yellow)),
                    Span::raw(file_list),
                ]),
                Line::from(vec![
                    Span::styled("Keys: ", Style::default().fg(Color::Cyan)),
                    Span::raw("1-4=KEY[0-3]  F1-F10=SW  r=reset  Space=pause  q=quit"),
                ]),
            ])
            .block(Block::default().borders(Borders::ALL));
            frame.render_widget(help, chunks[4]);
        })?;

        let elapsed = tick_start.elapsed();
        if elapsed < Duration::from_millis(TICK_RATE_MS) {
            std::thread::sleep(Duration::from_millis(TICK_RATE_MS) - elapsed);
        }
    }
}
