use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io::IsTerminal;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event as CEvent, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, ExecutableCommand};
use dupdupninja_core::db::SqliteScanStore;
use dupdupninja_core::models::ScanRootKind;
use dupdupninja_core::scan::{
    prescan, scan_to_sqlite_with_progress_and_totals, PrescanProgress, ScanCancelToken, ScanConfig,
    ScanProgress, ScanTotals,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Terminal;
use sysinfo::{Pid, ProcessesToUpdate, System};

mod web;

fn main() {
    if let Err(err) = real_main() {
        eprintln!("error: {err}");
        std::process::exit(2);
    }
}

fn real_main() -> dupdupninja_core::Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        print_help();
        return Ok(());
    };

    match cmd.as_str() {
        "--help" | "-h" | "help" => {
            print_help();
            Ok(())
        }
        "scan" => run_scan_command(&mut args),
        "matches" => run_matches_command(&mut args),
        "web" => {
            let mut port: u16 = 4455;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--port" => {
                        if let Some(val) = args.next() {
                            port = val.parse().map_err(|_| {
                                dupdupninja_core::Error::InvalidArgument(format!(
                                    "invalid --port value: {val}"
                                ))
                            })?;
                        }
                    }
                    _ => {
                        return Err(dupdupninja_core::Error::InvalidArgument(format!(
                            "unknown arg: {arg}"
                        )));
                    }
                }
            }
            web::run_web_server(port)?;
            Ok(())
        }
        _ => Err(dupdupninja_core::Error::InvalidArgument(format!(
            "unknown command: {cmd}"
        ))),
    }
}

fn print_help() {
    println!(
        r#"dupdupninja

USAGE:
  dupdupninja scan --root <path> [--db <fileset.ddn>] [--drive|--folder] [--single-threaded|--concurrent] [--capture-snapshots|--no-snapshots] [--snapshots-per-video <n>] [--snapshot-max-dim <px>]
  dupdupninja matches --db <sqlite_path> [--mode <all|similar|exact>] [--tui|--plain] [--max-files <n>] [--ahash <n>] [--dhash <n>] [--phash <n>]
  dupdupninja web [--port <port>]

NOTES:
  - Filesets are stored as standalone SQLite DBs (one per scan).
  - `scan` writes live progress in-place in the terminal (no scrolling log spam).
  - Snapshot capture is optional in CLI scan mode (`--capture-snapshots`).
  - Scan processing is concurrent by default.
  - UI crates are present but stubbed; the CLI is the initial entrypoint.
  - Web UI listens on http://127.0.0.1:4455 by default.
"#
    );
}

fn run_scan_command(args: &mut impl Iterator<Item = String>) -> dupdupninja_core::Result<()> {
    let mut root: Option<PathBuf> = None;
    let mut db: Option<PathBuf> = None;
    let mut root_kind = ScanRootKind::Folder;
    let mut concurrent_processing = true;
    let mut capture_snapshots = false;
    let mut snapshots_per_video: u32 = 3;
    let mut snapshot_max_dim: u32 = 1024;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" | "--path" => {
                let value = args.next().ok_or_else(|| {
                    dupdupninja_core::Error::InvalidArgument(
                        "missing value for --root <path>".to_string(),
                    )
                })?;
                root = Some(PathBuf::from(value));
            }
            "--db" => {
                let value = args.next().ok_or_else(|| {
                    dupdupninja_core::Error::InvalidArgument(
                        "missing value for --db <path>".to_string(),
                    )
                })?;
                db = Some(PathBuf::from(value));
            }
            "--drive" => root_kind = ScanRootKind::Drive,
            "--folder" => root_kind = ScanRootKind::Folder,
            "--single-threaded" => concurrent_processing = false,
            "--concurrent" => concurrent_processing = true,
            "--capture-snapshots" => capture_snapshots = true,
            "--no-snapshots" => capture_snapshots = false,
            "--snapshots-per-video" => {
                let value = args.next().ok_or_else(|| {
                    dupdupninja_core::Error::InvalidArgument(
                        "missing value for --snapshots-per-video <n>".to_string(),
                    )
                })?;
                snapshots_per_video = value.parse().map_err(|_| {
                    dupdupninja_core::Error::InvalidArgument(format!(
                        "invalid --snapshots-per-video value: {value}"
                    ))
                })?;
            }
            "--snapshot-max-dim" => {
                let value = args.next().ok_or_else(|| {
                    dupdupninja_core::Error::InvalidArgument(
                        "missing value for --snapshot-max-dim <px>".to_string(),
                    )
                })?;
                snapshot_max_dim = value.parse().map_err(|_| {
                    dupdupninja_core::Error::InvalidArgument(format!(
                        "invalid --snapshot-max-dim value: {value}"
                    ))
                })?;
            }
            _ => {
                return Err(dupdupninja_core::Error::InvalidArgument(format!(
                    "unknown arg: {arg}"
                )));
            }
        }
    }

    let root = root.ok_or_else(|| {
        dupdupninja_core::Error::InvalidArgument("missing --root <path>".to_string())
    })?;
    if capture_snapshots && snapshots_per_video == 0 {
        return Err(dupdupninja_core::Error::InvalidArgument(
            "--snapshots-per-video must be > 0 when --capture-snapshots is enabled".to_string(),
        ));
    }
    if capture_snapshots && snapshot_max_dim == 0 {
        return Err(dupdupninja_core::Error::InvalidArgument(
            "--snapshot-max-dim must be > 0 when --capture-snapshots is enabled".to_string(),
        ));
    }
    let db = db.unwrap_or_else(|| scan_db_path(&root));
    let store = SqliteScanStore::open(&db)?;
    let cfg = ScanConfig {
        root: root.clone(),
        root_kind,
        hash_files: true,
        perceptual_hashes: true,
        capture_snapshots,
        snapshots_per_video,
        snapshot_max_dim,
        concurrent_processing,
    };
    let snapshots_label = snapshot_settings_label(
        cfg.capture_snapshots,
        cfg.snapshots_per_video,
        cfg.snapshot_max_dim,
    );

    let mut tui = match ScanTui::start() {
        Ok(tui) => Some(tui),
        Err(err) => {
            eprintln!("warning: failed to initialize TUI ({err}); using plain progress output");
            None
        }
    };
    let mut plain_progress = if tui.is_none() {
        Some(TerminalProgress::new())
    } else {
        None
    };
    let visual_mode = detect_visual_mode();
    let mut ui_state = ScanUiState::new(
        root.clone(),
        db.clone(),
        root_kind,
        visual_mode,
        concurrent_processing,
        snapshots_label.clone(),
    );
    let cancel_token = ScanCancelToken::new();
    let mut cancel_watcher = CancelInputWatcher::start(cancel_token.clone(), tui.is_some());
    if let Some(ui) = tui.as_mut() {
        let _ = ui.render(&ui_state);
    } else {
        println!("root: {}", root.display());
        println!("root kind: {}", root_kind_label(root_kind));
        println!("db: {}", db.display());
        println!("snapshots: {snapshots_label}");
    }

    let prescan_result = prescan(&cfg, Some(&cancel_token), |update: &PrescanProgress| {
        if cancel_token.is_cancelled() {
            ui_state.on_cancel_requested();
        }
        if let Some(ui) = tui.as_mut() {
            ui_state.on_prescan_progress(update);
            if ui_state.should_render(false) {
                let _ = ui.render(&ui_state);
            }
        }
        if let Some(progress) = plain_progress.as_mut() {
            progress.draw_prescan(update);
        }
    });
    let totals = match prescan_result {
        Ok(totals) => {
            if let Some(progress) = plain_progress.as_mut() {
                progress.finish_line();
            }
            if let Some(ui) = tui.as_mut() {
                ui_state.on_prescan_done(totals);
                let _ = ui.render(&ui_state);
            } else {
                println!(
                    "prescan complete: {} files, {}",
                    totals.files,
                    human_bytes(totals.bytes)
                );
            }
            totals
        }
        Err(dupdupninja_core::Error::Cancelled) => {
            cancel_watcher.stop();
            if let Some(progress) = plain_progress.as_mut() {
                progress.finish_line();
            }
            if let Some(ui) = tui.as_mut() {
                ui_state.on_cancelled();
                let _ = ui.render(&ui_state);
            }
            drop(tui);
            println!("scan cancelled during prescan");
            println!("fileset: {}", db.display());
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let result = scan_to_sqlite_with_progress_and_totals(
        &cfg,
        &store,
        Some(&cancel_token),
        Some(totals),
        |update: &ScanProgress| {
            if cancel_token.is_cancelled() {
                ui_state.on_cancel_requested();
            }
            if let Some(ui) = tui.as_mut() {
                ui_state.on_scan_progress(update);
                if ui_state.should_render(false) {
                    let _ = ui.render(&ui_state);
                }
            }
            if let Some(progress) = plain_progress.as_mut() {
                progress.draw_scan(update);
            }
        },
    );
    if let Some(progress) = plain_progress.as_mut() {
        progress.finish_line();
    }
    cancel_watcher.stop();
    match result {
        Ok(result) => {
            if let Some(ui) = tui.as_mut() {
                ui_state.on_done(&result);
                if ui_state.should_render(true) {
                    let _ = ui.render(&ui_state);
                }
            }
            drop(tui);
            println!(
                "scan complete: {} files, {} hashed, {} skipped",
                result.stats.files_seen, result.stats.files_hashed, result.stats.files_skipped
            );
            println!("fileset: {}", db.display());
            Ok(())
        }
        Err(dupdupninja_core::Error::Cancelled) => {
            if let Some(ui) = tui.as_mut() {
                ui_state.on_cancelled();
                if ui_state.should_render(true) {
                    let _ = ui.render(&ui_state);
                }
            }
            drop(tui);
            println!("scan cancelled");
            println!("fileset (partial): {}", db.display());
            Ok(())
        }
        Err(err) => Err(err),
    }
}

struct CancelInputWatcher {
    stop_tx: Option<mpsc::Sender<()>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl CancelInputWatcher {
    fn start(cancel_token: ScanCancelToken, enabled: bool) -> Self {
        if !enabled {
            return Self {
                stop_tx: None,
                handle: None,
            };
        }

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let handle = thread::spawn(move || loop {
            if cancel_token.is_cancelled() {
                break;
            }

            match stop_rx.try_recv() {
                Ok(_) | Err(TryRecvError::Disconnected) => break,
                Err(TryRecvError::Empty) => {}
            }

            match event::poll(Duration::from_millis(100)) {
                Ok(true) => match event::read() {
                    Ok(CEvent::Key(key)) if key.kind == KeyEventKind::Press => match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                            cancel_token.cancel();
                            break;
                        }
                        _ => {}
                    },
                    Ok(_) => {}
                    Err(_) => {}
                },
                Ok(false) => {}
                Err(_) => {}
            }
        });

        Self {
            stop_tx: Some(stop_tx),
            handle: Some(handle),
        }
    }

    fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for CancelInputWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

struct ScanUiState {
    root: PathBuf,
    db: PathBuf,
    root_kind: ScanRootKind,
    snapshots_label: String,
    phase: &'static str,
    current_step: String,
    current_path: PathBuf,
    active_tasks: Vec<String>,
    files_seen: u64,
    files_hashed: u64,
    files_skipped: u64,
    total_files: u64,
    total_bytes: u64,
    prescan_files: u64,
    prescan_dirs: u64,
    prescan_bytes: u64,
    last_render: Instant,
    started_at: Instant,
    visual_mode: VisualMode,
    concurrent_processing: bool,
    process_system: System,
    process_pid: Option<Pid>,
    process_cpu_pct: Option<f32>,
    process_rss_bytes: Option<u64>,
    process_vmem_bytes: Option<u64>,
    process_threads: Option<usize>,
    process_children: Option<usize>,
    process_status: Option<String>,
    process_runtime_secs: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisualMode {
    Pretty,
    Plain,
}

impl ScanUiState {
    fn new(
        root: PathBuf,
        db: PathBuf,
        root_kind: ScanRootKind,
        visual_mode: VisualMode,
        concurrent_processing: bool,
        snapshots_label: String,
    ) -> Self {
        let process_pid = sysinfo::get_current_pid().ok();
        Self {
            root,
            db,
            root_kind,
            snapshots_label,
            phase: "prescan",
            current_step: "collecting totals".to_string(),
            current_path: PathBuf::new(),
            active_tasks: Vec::new(),
            files_seen: 0,
            files_hashed: 0,
            files_skipped: 0,
            total_files: 0,
            total_bytes: 0,
            prescan_files: 0,
            prescan_dirs: 0,
            prescan_bytes: 0,
            last_render: Instant::now()
                .checked_sub(Duration::from_millis(250))
                .unwrap_or_else(Instant::now),
            started_at: Instant::now(),
            visual_mode,
            concurrent_processing,
            process_system: System::new_all(),
            process_pid,
            process_cpu_pct: None,
            process_rss_bytes: None,
            process_vmem_bytes: None,
            process_threads: None,
            process_children: None,
            process_status: None,
            process_runtime_secs: None,
        }
    }

    fn on_prescan_progress(&mut self, progress: &PrescanProgress) {
        self.phase = "prescan";
        self.current_step = "collecting totals".to_string();
        self.current_path = progress.current_path.clone();
        self.prescan_files = progress.files_seen;
        self.prescan_dirs = progress.dirs_seen;
        self.prescan_bytes = progress.bytes_seen;
    }

    fn on_prescan_done(&mut self, totals: ScanTotals) {
        self.total_files = totals.files;
        self.total_bytes = totals.bytes;
    }

    fn on_scan_progress(&mut self, progress: &ScanProgress) {
        self.phase = "scan";
        self.current_step = progress
            .current_step
            .clone()
            .unwrap_or_else(|| "scan".to_string());
        self.current_path = progress.current_path.clone();
        self.active_tasks = progress
            .active_tasks
            .iter()
            .map(|task| format!("{} | {}", task.step, shorten_path(&task.path, 96)))
            .collect();
        self.files_seen = progress.files_seen;
        self.files_hashed = progress.files_hashed;
        self.files_skipped = progress.files_skipped;
        if progress.total_files > 0 {
            self.total_files = progress.total_files;
        }
        if progress.total_bytes > 0 {
            self.total_bytes = progress.total_bytes;
        }
    }

    fn on_done(&mut self, result: &dupdupninja_core::models::ScanResult) {
        self.phase = "done";
        self.current_step = "complete".to_string();
        self.active_tasks.clear();
        self.files_seen = result.stats.files_seen;
        self.files_hashed = result.stats.files_hashed;
        self.files_skipped = result.stats.files_skipped;
    }

    fn on_cancel_requested(&mut self) {
        self.phase = "cancel";
        self.current_step = "cancellation requested".to_string();
        self.active_tasks.clear();
    }

    fn on_cancelled(&mut self) {
        self.phase = "cancelled";
        self.current_step = "cancelled".to_string();
        self.active_tasks.clear();
    }

    fn should_render(&mut self, force: bool) -> bool {
        if force {
            self.refresh_process_metrics();
            self.last_render = Instant::now();
            return true;
        }
        let now = Instant::now();
        if now.duration_since(self.last_render) < Duration::from_millis(80) {
            return false;
        }
        self.refresh_process_metrics();
        self.last_render = now;
        true
    }

    fn progress_ratio(&self) -> f64 {
        if self.total_files == 0 {
            return 0.0;
        }
        (self.files_seen as f64 / self.total_files as f64).clamp(0.0, 1.0)
    }

    fn refresh_process_metrics(&mut self) {
        let Some(pid) = self.process_pid else {
            return;
        };

        self.process_system
            .refresh_processes(ProcessesToUpdate::All, true);

        if let Some(proc_) = self.process_system.process(pid) {
            self.process_cpu_pct = Some(proc_.cpu_usage());
            self.process_rss_bytes = Some(proc_.memory());
            self.process_vmem_bytes = Some(proc_.virtual_memory());
            self.process_threads = proc_.tasks().map(|tasks| tasks.len());
            self.process_children = Some(
                self.process_system
                    .processes()
                    .values()
                    .filter(|p| p.parent() == Some(pid))
                    .count(),
            );
            self.process_status = Some(format!("{:?}", proc_.status()).to_ascii_lowercase());
            self.process_runtime_secs = Some(proc_.run_time());
        }
    }
}

struct ScanTui {
    terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
}

impl ScanTui {
    fn start() -> std::io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        Ok(Self { terminal })
    }

    fn render(&mut self, state: &ScanUiState) -> std::io::Result<()> {
        self.terminal.draw(|frame| draw_scan_ui(frame, state))?;
        Ok(())
    }
}

impl Drop for ScanTui {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = self.terminal.backend_mut().execute(LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn draw_scan_ui(frame: &mut ratatui::Frame<'_>, state: &ScanUiState) {
    let pretty = state.visual_mode == VisualMode::Pretty;
    let border_style = if pretty {
        Style::default().fg(Color::Rgb(90, 100, 120))
    } else {
        Style::default()
    };
    let accent = if pretty {
        Style::default()
            .fg(Color::Rgb(72, 187, 255))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    let section_title = |name: &'static str| -> Line<'static> {
        if pretty {
            Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(name, accent),
                Span::styled(" ", Style::default()),
            ])
        } else {
            Line::from(name)
        }
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let header = Paragraph::new(if pretty {
        Line::from(vec![
            Span::styled(
                "dupdupninja ",
                Style::default().fg(Color::Rgb(156, 163, 175)),
            ),
            Span::styled("CLI Scan", accent),
            Span::styled("  [ratatui]", Style::default().fg(Color::Rgb(34, 197, 94))),
        ])
    } else {
        Line::from("dupdupninja CLI scan")
    })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(if pretty {
                border::ROUNDED
            } else {
                border::PLAIN
            })
            .border_style(border_style)
            .title(section_title("Scan")),
    );
    frame.render_widget(header, chunks[0]);

    let metadata = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Root: ", Style::default().fg(Color::Gray)),
            Span::raw(state.root.display().to_string()),
        ]),
        Line::from(vec![
            Span::styled("Root kind: ", Style::default().fg(Color::Gray)),
            Span::raw(root_kind_label(state.root_kind)),
        ]),
        Line::from(vec![
            Span::styled("DB: ", Style::default().fg(Color::Gray)),
            Span::raw(state.db.display().to_string()),
        ]),
        Line::from(vec![
            Span::styled("Snapshots: ", Style::default().fg(Color::Gray)),
            Span::styled(
                state.snapshots_label.clone(),
                if state.snapshots_label == "disabled" {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Green)
                },
            ),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(if pretty {
                border::ROUNDED
            } else {
                border::PLAIN
            })
            .border_style(border_style)
            .title(section_title("Settings")),
    )
    .wrap(Wrap { trim: false });
    frame.render_widget(metadata, chunks[1]);

    let pct = state.progress_ratio() * 100.0;
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(if pretty {
                    border::ROUNDED
                } else {
                    border::PLAIN
                })
                .border_style(border_style)
                .title(section_title("Progress")),
        )
        .ratio(state.progress_ratio())
        .use_unicode(pretty)
        .gauge_style(if pretty {
            Style::default()
                .fg(Color::Rgb(59, 130, 246))
                .bg(Color::Rgb(30, 41, 59))
        } else {
            Style::default().fg(Color::Cyan)
        })
        .label(format!("{pct:>5.1}% ({})", state.phase));
    frame.render_widget(gauge, chunks[2]);

    let mut detail_lines = vec![Line::from(vec![
        Span::styled("Files: ", Style::default().fg(Color::Gray)),
        Span::raw(format!("{} / {}", state.files_seen, state.total_files)),
        Span::styled("  Hashed: ", Style::default().fg(Color::Gray)),
        Span::styled(
            state.files_hashed.to_string(),
            Style::default().fg(Color::Green),
        ),
        Span::styled("  Skipped: ", Style::default().fg(Color::Gray)),
        Span::styled(
            state.files_skipped.to_string(),
            Style::default().fg(Color::Red),
        ),
    ])];

    if state.concurrent_processing {
        detail_lines.push(Line::from(vec![Span::styled(
            "Active tasks:",
            Style::default().fg(Color::Gray),
        )]));
        if state.active_tasks.is_empty() {
            detail_lines.push(Line::from("  -"));
        } else {
            for task in &state.active_tasks {
                if let Some((step, path)) = task.split_once('|') {
                    let step_style = active_task_step_style(step.trim(), pretty);
                    detail_lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(step.trim().to_string(), step_style),
                        Span::styled(" | ", Style::default().fg(Color::Gray)),
                        Span::raw(path.trim().to_string()),
                    ]));
                } else {
                    detail_lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            task.clone(),
                            if pretty {
                                Style::default().fg(Color::Rgb(251, 191, 36))
                            } else {
                                Style::default()
                            },
                        ),
                    ]));
                }
            }
        }
    } else {
        detail_lines.push(Line::from(vec![
            Span::styled("Step: ", Style::default().fg(Color::Gray)),
            Span::styled(
                state.current_step.clone(),
                if pretty {
                    Style::default().fg(Color::Rgb(251, 191, 36))
                } else {
                    Style::default()
                },
            ),
        ]));
        detail_lines.push(Line::from(vec![
            Span::styled("Current: ", Style::default().fg(Color::Gray)),
            Span::raw(shorten_path(&state.current_path, 110)),
        ]));
    }

    detail_lines.push(Line::from(vec![
        Span::styled("Prescan: ", Style::default().fg(Color::Gray)),
        Span::raw(format!(
            "files {} | dirs {} | bytes {}",
            state.prescan_files,
            state.prescan_dirs,
            human_bytes(state.prescan_bytes)
        )),
    ]));
    detail_lines.push(Line::from(vec![
        Span::styled("Totals: ", Style::default().fg(Color::Gray)),
        Span::raw(format!(
            "{} | elapsed {}",
            human_bytes(state.total_bytes),
            human_elapsed(state.started_at.elapsed())
        )),
    ]));

    let details = Paragraph::new(detail_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(if pretty {
                    border::ROUNDED
                } else {
                    border::PLAIN
                })
                .border_style(border_style)
                .title(section_title("Details")),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(details, chunks[3]);

    let footer = Paragraph::new("Press q or Esc to cancel scan (or Ctrl+C to abort process).")
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(if pretty {
                    border::ROUNDED
                } else {
                    border::PLAIN
                })
                .border_style(border_style)
                .title(section_title("Control")),
        );
    frame.render_widget(footer, chunks[4]);

    let cpu = state
        .process_cpu_pct
        .map(|v| format!("{v:.1}%"))
        .unwrap_or_else(|| "n/a".to_string());
    let rss = state
        .process_rss_bytes
        .map(human_bytes)
        .unwrap_or_else(|| "n/a".to_string());
    let vmem = state
        .process_vmem_bytes
        .map(human_bytes)
        .unwrap_or_else(|| "n/a".to_string());
    let threads = state
        .process_threads
        .map(|v| v.to_string())
        .unwrap_or_else(|| "n/a".to_string());
    let children = state
        .process_children
        .map(|v| v.to_string())
        .unwrap_or_else(|| "n/a".to_string());
    let status = state
        .process_status
        .clone()
        .unwrap_or_else(|| "n/a".to_string());
    let runtime = state
        .process_runtime_secs
        .map(|secs| human_elapsed(Duration::from_secs(secs)))
        .unwrap_or_else(|| "n/a".to_string());
    let process_info = Paragraph::new(format!(
        "cpu {cpu} | rss {rss} | vmem {vmem} | threads {threads} | child procs {children} | status {status} | runtime {runtime}"
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(if pretty {
                border::ROUNDED
            } else {
                border::PLAIN
            })
            .border_style(border_style)
            .title(section_title("Process")),
    );
    frame.render_widget(process_info, chunks[5]);
}

fn active_task_step_style(step: &str, pretty: bool) -> Style {
    if !pretty {
        return Style::default();
    }
    match step {
        "blake3" => Style::default().fg(Color::Rgb(34, 197, 94)),
        "sha256" => Style::default().fg(Color::Rgb(59, 130, 246)),
        "ffprobe metadata" => Style::default().fg(Color::Rgb(251, 191, 36)),
        "ahash/dhash/phash" => Style::default().fg(Color::Rgb(244, 114, 182)),
        "video snapshots" => Style::default().fg(Color::Rgb(168, 85, 247)),
        "file type" => Style::default().fg(Color::Rgb(16, 185, 129)),
        "metadata" => Style::default().fg(Color::Rgb(148, 163, 184)),
        "done" => Style::default().fg(Color::Rgb(250, 204, 21)),
        _ => Style::default().fg(Color::Rgb(251, 191, 36)),
    }
}

fn detect_visual_mode() -> VisualMode {
    let term = std::env::var("TERM").unwrap_or_default();
    let no_color = std::env::var_os("NO_COLOR").is_some();
    if no_color || term == "dumb" {
        VisualMode::Plain
    } else {
        VisualMode::Pretty
    }
}

fn scan_db_path(root: &Path) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let name = sanitize_fileset_name(root);
    let file_name = format!("{name}-{ts}.ddn");

    let mut base = default_fileset_dir();
    if std::fs::create_dir_all(&base).is_err() {
        let mut fallback = std::env::temp_dir();
        fallback.push(file_name);
        return fallback;
    }
    base.push(file_name);
    base
}

fn sanitize_fileset_name(root: &Path) -> String {
    let raw = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("fileset");
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() || ch == '.' {
            out.push('-');
        }
    }
    if out.is_empty() {
        "fileset".to_string()
    } else {
        out
    }
}

fn default_fileset_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    let mut path = base;
    path.push("dupdupninja");
    path.push("filesets");
    path
}

fn human_elapsed(d: Duration) -> String {
    let total = d.as_secs();
    let days = total / 86_400;
    let hours = (total % 86_400) / 3_600;
    let mins = (total % 3_600) / 60;
    let secs = total % 60;

    if days > 0 {
        format!("{days}d {hours}h {mins}m {secs}s")
    } else if hours > 0 {
        format!("{hours}h {mins}m {secs}s")
    } else if mins > 0 {
        format!("{mins}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

fn root_kind_label(root_kind: ScanRootKind) -> &'static str {
    match root_kind {
        ScanRootKind::Folder => "folder",
        ScanRootKind::Drive => "drive",
    }
}

fn snapshot_settings_label(
    capture_snapshots: bool,
    snapshots_per_video: u32,
    snapshot_max_dim: u32,
) -> String {
    if !capture_snapshots {
        "disabled".to_string()
    } else {
        format!("{snapshots_per_video} per video @ {snapshot_max_dim}px")
    }
}

struct TerminalProgress {
    last_render: Instant,
    last_width: usize,
}

impl TerminalProgress {
    fn new() -> Self {
        Self {
            last_render: Instant::now()
                .checked_sub(Duration::from_millis(250))
                .unwrap_or_else(Instant::now),
            last_width: 0,
        }
    }

    fn draw_prescan(&mut self, progress: &PrescanProgress) {
        if !self.should_render(false) {
            return;
        }
        let current = shorten_path(&progress.current_path, 44);
        let line = format!(
            "prescan | files {:>8} | dirs {:>7} | bytes {:>10} | {}",
            progress.files_seen,
            progress.dirs_seen,
            human_bytes(progress.bytes_seen),
            current
        );
        self.render_line(&line);
    }

    fn draw_scan(&mut self, progress: &ScanProgress) {
        if !self.should_render(false) {
            return;
        }
        let pct = if progress.total_files > 0 {
            (progress.files_seen as f64 / progress.total_files as f64) * 100.0
        } else {
            0.0
        };
        let step = progress.current_step.as_deref().unwrap_or("scan");
        let current = shorten_path(&progress.current_path, 38);
        let mut line = format!(
            "scan {:>5.1}% | files {:>8}/{:<8} | hashed {:>8} | skipped {:>6} | {} | {}",
            pct,
            progress.files_seen,
            progress.total_files,
            progress.files_hashed,
            progress.files_skipped,
            step,
            current
        );
        if let Some(task) = progress.active_tasks.first() {
            line.push_str(&format!(
                " | active {}: {}",
                task.step,
                shorten_path(&task.path, 24)
            ));
        }
        self.render_line(&line);
    }

    fn finish_line(&mut self) {
        self.should_render(true);
        self.render_line("");
        eprintln!();
        self.last_width = 0;
    }

    fn should_render(&mut self, force: bool) -> bool {
        if force {
            self.last_render = Instant::now();
            return true;
        }
        let now = Instant::now();
        if now.duration_since(self.last_render) < Duration::from_millis(80) {
            return false;
        }
        self.last_render = now;
        true
    }

    fn render_line(&mut self, line: &str) {
        let mut stderr = std::io::stderr();
        let clear_pad = self.last_width.saturating_sub(line.len());
        let _ = write!(stderr, "\r{line}{:clear_pad$}", "");
        let _ = stderr.flush();
        self.last_width = line.len();
    }
}

fn shorten_path(path: &Path, max: usize) -> String {
    let full = path.display().to_string();
    let char_count = full.chars().count();
    if char_count <= max {
        return full;
    }
    let keep = max.saturating_sub(1);
    let tail: String = full
        .chars()
        .rev()
        .take(keep)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("...{tail}")
}

fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn run_matches_command(args: &mut impl Iterator<Item = String>) -> dupdupninja_core::Result<()> {
    let mut db: Option<PathBuf> = None;
    let mut max_files: usize = 500;
    let mut mode = MatchMode::All;
    let mut use_tui: Option<bool> = None;
    let mut thresholds = SimilarityThresholds {
        ahash: 10,
        dhash: 10,
        phash: 8,
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db = args.next().map(PathBuf::from),
            "--mode" => {
                let value = args.next().ok_or_else(|| {
                    dupdupninja_core::Error::InvalidArgument(
                        "missing value for --mode <all|similar|exact>".to_string(),
                    )
                })?;
                mode = MatchMode::parse(&value)?;
            }
            "--all" => mode = MatchMode::All,
            "--similar" => mode = MatchMode::Similar,
            "--exact" => mode = MatchMode::Exact,
            "--tui" => use_tui = Some(true),
            "--plain" => use_tui = Some(false),
            "--max-files" => {
                if let Some(val) = args.next() {
                    max_files = val.parse().map_err(|_| {
                        dupdupninja_core::Error::InvalidArgument(format!(
                            "invalid --max-files value: {val}"
                        ))
                    })?;
                }
            }
            "--ahash" => {
                if let Some(val) = args.next() {
                    thresholds.ahash = val.parse().map_err(|_| {
                        dupdupninja_core::Error::InvalidArgument(format!(
                            "invalid --ahash value: {val}"
                        ))
                    })?;
                }
            }
            "--dhash" => {
                if let Some(val) = args.next() {
                    thresholds.dhash = val.parse().map_err(|_| {
                        dupdupninja_core::Error::InvalidArgument(format!(
                            "invalid --dhash value: {val}"
                        ))
                    })?;
                }
            }
            "--phash" => {
                if let Some(val) = args.next() {
                    thresholds.phash = val.parse().map_err(|_| {
                        dupdupninja_core::Error::InvalidArgument(format!(
                            "invalid --phash value: {val}"
                        ))
                    })?;
                }
            }
            _ => {
                return Err(dupdupninja_core::Error::InvalidArgument(format!(
                    "unknown arg: {arg}"
                )));
            }
        }
    }

    let db = db.ok_or_else(|| {
        dupdupninja_core::Error::InvalidArgument("missing --db <path>".to_string())
    })?;
    let store = SqliteScanStore::open(&db)?;

    let load_exact = mode.includes_exact();
    let load_similar = mode.includes_similar();
    let exact = if load_exact {
        collect_exact_duplicate_groups(&store, max_files)?
    } else {
        Vec::new()
    };
    let similar = if load_similar {
        collect_similar_groups(&store, max_files, thresholds)?
    } else {
        Vec::new()
    };
    if exact.is_empty() && similar.is_empty() {
        println!("no matches found");
        return Ok(());
    }

    let use_tui = use_tui.unwrap_or_else(|| {
        std::io::stdout().is_terminal()
            && std::io::stderr().is_terminal()
            && detect_visual_mode() == VisualMode::Pretty
    });

    if use_tui {
        let mut state = MatchesUiState::new(
            mode,
            thresholds,
            max_files,
            load_exact,
            load_similar,
            exact,
            similar,
        );
        if let Err(err) = run_matches_tui(&mut state, &store) {
            eprintln!("warning: failed to run matches TUI ({err}); falling back to plain output");
            let groups = state.filtered_groups();
            print_matches_plain(mode, thresholds, &groups);
        }
    } else {
        let state = MatchesUiState::new(
            mode,
            thresholds,
            max_files,
            load_exact,
            load_similar,
            exact,
            similar,
        );
        let groups = state.filtered_groups();
        print_matches_plain(mode, thresholds, &groups);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatchMode {
    All,
    Similar,
    Exact,
}

impl MatchMode {
    fn parse(value: &str) -> dupdupninja_core::Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "all" => Ok(Self::All),
            "similar" => Ok(Self::Similar),
            "exact" => Ok(Self::Exact),
            _ => Err(dupdupninja_core::Error::InvalidArgument(format!(
                "invalid --mode value: {value} (expected all|similar|exact)"
            ))),
        }
    }

    fn includes_exact(self) -> bool {
        matches!(self, Self::All | Self::Exact)
    }

    fn includes_similar(self) -> bool {
        matches!(self, Self::All | Self::Similar)
    }
}

#[derive(Clone, Copy, Debug)]
struct SimilarityThresholds {
    ahash: u32,
    dhash: u32,
    phash: u32,
}

#[derive(Clone, Copy, Debug)]
struct HashScore {
    distance: u32,
    confidence_pct: f64,
}

#[derive(Clone, Copy, Debug)]
struct SimilarityScores {
    overall_pct: f64,
    phash: Option<HashScore>,
    dhash: Option<HashScore>,
    ahash: Option<HashScore>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatchGroupKind {
    Exact,
    Similar,
}

#[derive(Clone, Debug)]
struct MatchEntry {
    file_id: i64,
    path: PathBuf,
    size_bytes: u64,
    modified_at: Option<SystemTime>,
    file_type: Option<String>,
    ffmpeg_metadata: Option<String>,
    detail: Option<String>,
}

#[derive(Clone, Debug)]
struct MatchGroup {
    kind: MatchGroupKind,
    title: String,
    summary: String,
    confidence_pct: f64,
    sort_size_bytes: u64,
    entries: Vec<MatchEntry>,
}

fn collect_exact_duplicate_groups(
    store: &SqliteScanStore,
    max_files: usize,
) -> dupdupninja_core::Result<Vec<MatchGroup>> {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum ExactKey {
        Blake3([u8; 32]),
        Sha256([u8; 32]),
    }

    let files = store.list_files(max_files, 0)?;
    if files.len() == max_files {
        eprintln!(
            "warning: reached --max-files limit for exact matching; results may be incomplete"
        );
    }

    let mut groups: HashMap<ExactKey, Vec<usize>> = HashMap::new();
    for (idx, file) in files.iter().enumerate() {
        let key = if let Some(hash) = file.blake3 {
            Some(ExactKey::Blake3(hash))
        } else {
            file.sha256.map(ExactKey::Sha256)
        };
        if let Some(key) = key {
            groups.entry(key).or_default().push(idx);
        }
    }

    let mut group_list: Vec<(ExactKey, Vec<usize>)> = groups
        .into_iter()
        .filter(|(_, members)| members.len() > 1)
        .collect();
    group_list.sort_by(|a, b| {
        let a_size =
            a.1.first()
                .map(|idx| files[*idx].size_bytes)
                .unwrap_or_default();
        let b_size =
            b.1.first()
                .map(|idx| files[*idx].size_bytes)
                .unwrap_or_default();
        b_size.cmp(&a_size).then_with(|| b.1.len().cmp(&a.1.len()))
    });

    let mut out = Vec::new();
    for (key, members) in group_list {
        let (algo, short_hash) = match key {
            ExactKey::Blake3(hash) => ("blake3", short_hash_hex(&hash)),
            ExactKey::Sha256(hash) => ("sha256", short_hash_hex(&hash)),
        };
        let mut entries = Vec::with_capacity(members.len());
        let mut group_size = 0_u64;
        for member_idx in members {
            let row = &files[member_idx];
            group_size = row.size_bytes;
            entries.push(match_entry_from_row(row, None));
        }
        out.push(MatchGroup {
            kind: MatchGroupKind::Exact,
            title: format!("exact {} [{}:{}]", entries.len(), algo, short_hash),
            summary: format!("{} files | {}", entries.len(), human_bytes(group_size)),
            confidence_pct: 100.0,
            sort_size_bytes: group_size,
            entries,
        });
    }
    Ok(out)
}

fn collect_similar_groups(
    store: &SqliteScanStore,
    max_files: usize,
    thresholds: SimilarityThresholds,
) -> dupdupninja_core::Result<Vec<MatchGroup>> {
    let files = store.list_files_with_hashes(max_files, 0)?;
    if files.len() == max_files {
        eprintln!(
            "warning: reached --max-files limit for similar matching; results may be incomplete"
        );
    }

    let mut groups: Vec<(usize, Vec<(usize, SimilarityScores)>)> = Vec::new();
    let mut assigned = vec![false; files.len()];

    for i in 0..files.len() {
        if assigned[i] {
            continue;
        }
        let anchor = &files[i];
        let mut members = Vec::new();

        for j in (i + 1)..files.len() {
            if assigned[j] {
                continue;
            }
            let scores = similarity_scores(anchor, &files[j]);
            if passes_similarity(scores, thresholds) {
                members.push((j, scores));
            }
        }

        if !members.is_empty() {
            assigned[i] = true;
            for (member_idx, _) in &members {
                assigned[*member_idx] = true;
            }
            members.sort_by(|a, b| b.1.overall_pct.total_cmp(&a.1.overall_pct));
            groups.push((i, members));
        }
    }

    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    let mut out = Vec::new();
    for (anchor_idx, members) in groups {
        let mut entries = Vec::with_capacity(members.len() + 1);
        let anchor_size = files[anchor_idx].size_bytes;
        entries.push(match_entry_from_row(
            &files[anchor_idx],
            Some("reference".to_string()),
        ));
        let mut best = 0.0_f64;
        for (member_idx, scores) in members {
            best = best.max(scores.overall_pct);
            entries.push(match_entry_from_row(
                &files[member_idx],
                Some(format!(
                    "confidence {:.1}% | phash {} | dhash {} | ahash {}",
                    scores.overall_pct,
                    format_hash_score(scores.phash),
                    format_hash_score(scores.dhash),
                    format_hash_score(scores.ahash)
                )),
            ));
        }
        out.push(MatchGroup {
            kind: MatchGroupKind::Similar,
            title: format!("similar {} (best {:.1}%)", entries.len(), best),
            summary: format!("{} files | ref {}", entries.len(), human_bytes(anchor_size)),
            confidence_pct: best.min(99.99),
            sort_size_bytes: anchor_size,
            entries,
        });
    }
    Ok(out)
}

fn match_entry_from_row(
    row: &dupdupninja_core::models::FileListRow,
    detail: Option<String>,
) -> MatchEntry {
    MatchEntry {
        file_id: row.id,
        path: row.path.clone(),
        size_bytes: row.size_bytes,
        modified_at: row.modified_at,
        file_type: row.file_type.clone(),
        ffmpeg_metadata: row.ffmpeg_metadata.clone(),
        detail,
    }
}

fn print_matches_plain(mode: MatchMode, thresholds: SimilarityThresholds, groups: &[MatchGroup]) {
    if mode.includes_exact() {
        let exact_count = groups
            .iter()
            .filter(|g| g.kind == MatchGroupKind::Exact)
            .count();
        if exact_count > 0 {
            println!("Exact duplicate groups (blake3/sha256):");
            let mut idx = 1usize;
            for group in groups.iter().filter(|g| g.kind == MatchGroupKind::Exact) {
                println!(
                    "Group {}: {} | {} | confidence {:.2}%",
                    idx, group.title, group.summary, group.confidence_pct
                );
                for entry in &group.entries {
                    println!("  {}", entry.path.display());
                }
                idx += 1;
            }
        }
    }
    if mode.includes_similar() {
        let similar_count = groups
            .iter()
            .filter(|g| g.kind == MatchGroupKind::Similar)
            .count();
        if similar_count > 0 {
            if mode.includes_exact() {
                println!();
            }
            println!(
                "Similar groups (pHash primary, thresholds: phash<= {}, dhash<= {}, ahash<= {}):",
                thresholds.phash, thresholds.dhash, thresholds.ahash
            );
            let mut idx = 1usize;
            for group in groups.iter().filter(|g| g.kind == MatchGroupKind::Similar) {
                println!(
                    "Group {}: {} | {} | confidence {:.2}%",
                    idx, group.title, group.summary, group.confidence_pct
                );
                for entry in &group.entries {
                    if let Some(detail) = &entry.detail {
                        println!("  {} ({detail})", entry.path.display());
                    } else {
                        println!("  {}", entry.path.display());
                    }
                }
                idx += 1;
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum VisibleRow {
    Group(usize),
    Entry(usize, usize),
}

struct MatchesUiState {
    mode: MatchMode,
    thresholds: SimilarityThresholds,
    max_files: usize,
    load_exact: bool,
    load_similar: bool,
    exact: Vec<MatchGroup>,
    similar: Vec<MatchGroup>,
    expanded: HashSet<usize>,
    selected: usize,
    min_confidence: f64,
    show_full_ffmpeg_metadata: bool,
    status_message: Option<String>,
}

#[derive(Clone, Debug)]
struct SelectedEntryContext {
    path: PathBuf,
    reference_path: PathBuf,
    is_reference: bool,
}

impl MatchesUiState {
    fn new(
        mode: MatchMode,
        thresholds: SimilarityThresholds,
        max_files: usize,
        load_exact: bool,
        load_similar: bool,
        exact: Vec<MatchGroup>,
        similar: Vec<MatchGroup>,
    ) -> Self {
        Self {
            mode,
            thresholds,
            max_files,
            load_exact,
            load_similar,
            exact,
            similar,
            expanded: HashSet::new(),
            selected: 0,
            min_confidence: 0.0,
            show_full_ffmpeg_metadata: false,
            status_message: None,
        }
    }

    fn filtered_groups(&self) -> Vec<MatchGroup> {
        let mut out = Vec::new();
        if self.mode.includes_exact() {
            out.extend(self.exact.clone());
        }
        if self.mode.includes_similar() {
            out.extend(self.similar.clone());
        }
        if self.min_confidence > 0.0 {
            out.retain(|g| g.confidence_pct >= self.min_confidence);
        }
        out.sort_by(|a, b| {
            b.confidence_pct
                .total_cmp(&a.confidence_pct)
                .then_with(|| b.sort_size_bytes.cmp(&a.sort_size_bytes))
                .then_with(|| b.entries.len().cmp(&a.entries.len()))
                .then_with(|| a.title.cmp(&b.title))
        });
        out
    }

    fn visible_rows(&self) -> Vec<VisibleRow> {
        let groups = self.filtered_groups();
        let mut rows = Vec::new();
        for (gidx, group) in groups.iter().enumerate() {
            let _ = group;
            rows.push(VisibleRow::Group(gidx));
            if self.expanded.contains(&gidx) {
                for eidx in 0..groups[gidx].entries.len() {
                    rows.push(VisibleRow::Entry(gidx, eidx));
                }
            }
        }
        rows
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_rows().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    fn move_up(&mut self) {
        self.clamp_selection();
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn move_down(&mut self) {
        self.clamp_selection();
        let len = self.visible_rows().len();
        if self.selected + 1 < len {
            self.selected += 1;
        }
    }

    fn selected_row(&self) -> Option<VisibleRow> {
        let rows = self.visible_rows();
        rows.get(self.selected).copied()
    }

    fn selected_entry_context(&self) -> Option<SelectedEntryContext> {
        let rows = self.visible_rows();
        let groups = self.filtered_groups();
        let row = rows.get(self.selected)?;
        let VisibleRow::Entry(group_idx, entry_idx) = row else {
            return None;
        };
        let group = groups.get(*group_idx)?;
        let entry = group.entries.get(*entry_idx)?;
        let reference = group.entries.first()?;
        Some(SelectedEntryContext {
            path: entry.path.clone(),
            reference_path: reference.path.clone(),
            is_reference: *entry_idx == 0,
        })
    }

    fn toggle_expand_selected(&mut self) {
        if let Some(VisibleRow::Group(gidx)) = self.selected_row() {
            if self.expanded.contains(&gidx) {
                self.expanded.remove(&gidx);
            } else {
                self.expanded.insert(gidx);
            }
            self.clamp_selection();
        }
    }

    fn collapse_selected(&mut self) {
        match self.selected_row() {
            Some(VisibleRow::Group(gidx)) => {
                self.expanded.remove(&gidx);
            }
            Some(VisibleRow::Entry(gidx, _)) => {
                self.expanded.remove(&gidx);
            }
            None => {}
        }
        self.clamp_selection();
    }

    fn set_mode(&mut self, mode: MatchMode) {
        let unsupported = match mode {
            MatchMode::All => !(self.load_exact && self.load_similar),
            MatchMode::Exact => !self.load_exact,
            MatchMode::Similar => !self.load_similar,
        };
        if unsupported {
            self.set_status_message(
                "mode not available for this run; restart matches command in that mode",
            );
            return;
        }
        self.mode = mode;
        self.expanded.clear();
        self.selected = 0;
        self.show_full_ffmpeg_metadata = false;
    }

    fn adjust_min_confidence(&mut self, delta: f64) {
        self.min_confidence = (self.min_confidence + delta).clamp(0.0, 100.0);
        self.selected = 0;
        self.expanded.clear();
        self.show_full_ffmpeg_metadata = false;
        self.clamp_selection();
    }

    fn clear_filters(&mut self) {
        self.min_confidence = 0.0;
        self.selected = 0;
        self.expanded.clear();
        self.show_full_ffmpeg_metadata = false;
        self.clamp_selection();
    }

    fn toggle_ffmpeg_details(&mut self) {
        self.show_full_ffmpeg_metadata = !self.show_full_ffmpeg_metadata;
    }

    fn set_status_message(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
    }

    fn reload_groups(&mut self, store: &SqliteScanStore) -> dupdupninja_core::Result<()> {
        if self.load_exact {
            self.exact = collect_exact_duplicate_groups(store, self.max_files)?;
        }
        if self.load_similar {
            self.similar = collect_similar_groups(store, self.max_files, self.thresholds)?;
        }
        self.expanded.clear();
        self.selected = 0;
        self.show_full_ffmpeg_metadata = false;
        self.clamp_selection();
        Ok(())
    }
}

fn run_matches_tui(
    state: &mut MatchesUiState,
    store: &SqliteScanStore,
) -> dupdupninja_core::Result<()> {
    let mut terminal = MatchesTui::start()?;
    loop {
        terminal.render(state)?;
        if event::poll(Duration::from_millis(120))? {
            let CEvent::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Down | KeyCode::Char('j') => state.move_down(),
                KeyCode::Up | KeyCode::Char('k') => state.move_up(),
                KeyCode::Enter => {
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        if state.selected_entry_context().is_some() {
                            state.toggle_ffmpeg_details();
                        } else {
                            state.toggle_expand_selected();
                        }
                    } else if let Some(selected) = state.selected_entry_context() {
                        match reveal_in_file_browser(&selected.path) {
                            Ok(()) => state.set_status_message(format!(
                                "opened file manager for {}",
                                selected.path.display()
                            )),
                            Err(err) => state.set_status_message(format!(
                                "failed to open file manager for {}: {err}",
                                selected.path.display()
                            )),
                        }
                    } else {
                        state.toggle_expand_selected();
                    }
                }
                KeyCode::Char(' ') | KeyCode::Right => state.toggle_expand_selected(),
                KeyCode::Left => state.collapse_selected(),
                KeyCode::Char('o') | KeyCode::Char('O') => {
                    if let Some(selected) = state.selected_entry_context() {
                        match reveal_in_file_browser(&selected.path) {
                            Ok(()) => state.set_status_message(format!(
                                "opened file manager for {}",
                                selected.path.display()
                            )),
                            Err(err) => state.set_status_message(format!(
                                "failed to open file manager for {}: {err}",
                                selected.path.display()
                            )),
                        }
                    }
                }
                KeyCode::Delete => {
                    let action = if key.modifiers.contains(KeyModifiers::SHIFT) {
                        FileAction::DeletePermanently
                    } else {
                        FileAction::MoveToTrash
                    };
                    match handle_selected_file_action(state, store, action) {
                        Ok(()) => {}
                        Err(err) => state.set_status_message(format!("action failed: {err}")),
                    }
                }
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    match handle_selected_file_action(state, store, FileAction::ReplaceWithSymlink)
                    {
                        Ok(()) => {}
                        Err(err) => state.set_status_message(format!("action failed: {err}")),
                    }
                }
                KeyCode::Char('a') => state.set_mode(MatchMode::All),
                KeyCode::Char('e') => state.set_mode(MatchMode::Exact),
                KeyCode::Char('s') => state.set_mode(MatchMode::Similar),
                KeyCode::Char('+') | KeyCode::Char('=') => state.adjust_min_confidence(5.0),
                KeyCode::Char('-') | KeyCode::Char('_') => state.adjust_min_confidence(-5.0),
                KeyCode::Char('0') => state.clear_filters(),
                _ => {}
            };
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum FileAction {
    MoveToTrash,
    DeletePermanently,
    ReplaceWithSymlink,
}

fn handle_selected_file_action(
    state: &mut MatchesUiState,
    store: &SqliteScanStore,
    action: FileAction,
) -> dupdupninja_core::Result<()> {
    let Some(selected) = state.selected_entry_context() else {
        state.set_status_message("select a file entry first");
        return Ok(());
    };

    match action {
        FileAction::MoveToTrash => {
            move_path_to_trash(&selected.path).map_err(dupdupninja_core::Error::Io)?;
            let _ = store.delete_file_by_path(&selected.path)?;
            state.reload_groups(store)?;
            state.set_status_message(format!("moved to trash: {}", selected.path.display()));
        }
        FileAction::DeletePermanently => {
            delete_path_permanently(&selected.path).map_err(dupdupninja_core::Error::Io)?;
            let _ = store.delete_file_by_path(&selected.path)?;
            state.reload_groups(store)?;
            state.set_status_message(format!("deleted permanently: {}", selected.path.display()));
        }
        FileAction::ReplaceWithSymlink => {
            if selected.is_reference {
                state.set_status_message(
                    "selected file is the group reference; choose another entry to replace",
                );
                return Ok(());
            }
            if selected.path == selected.reference_path {
                state.set_status_message("cannot symlink a file to itself");
                return Ok(());
            }
            replace_with_symlink_and_trash(&selected.path, &selected.reference_path)
                .map_err(dupdupninja_core::Error::Io)?;
            let _ = store.delete_file_by_path(&selected.path)?;
            state.reload_groups(store)?;
            state.set_status_message(format!(
                "replaced with symlink: {} -> {}",
                selected.path.display(),
                selected.reference_path.display()
            ));
        }
    }
    Ok(())
}

fn delete_path_permanently(path: &Path) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn replace_with_symlink_and_trash(
    duplicate_path: &Path,
    target_path: &Path,
) -> std::io::Result<()> {
    let backup = unique_backup_path(duplicate_path);
    fs::rename(duplicate_path, &backup)?;
    if let Err(err) = create_file_symlink(target_path, duplicate_path) {
        let _ = fs::rename(&backup, duplicate_path);
        return Err(err);
    }
    if let Err(err) = move_path_to_trash(&backup) {
        let _ = delete_path_permanently(&backup);
        return Err(std::io::Error::new(
            err.kind(),
            format!(
                "created symlink but failed to move original to trash: {err}; deleted backup permanently"
            ),
        ));
    }
    Ok(())
}

fn unique_backup_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    for attempt in 0..10_000_u32 {
        let suffix = if attempt == 0 {
            ".ddn-replace-trash.tmp".to_string()
        } else {
            format!(".ddn-replace-trash-{attempt}.tmp")
        };
        let candidate = parent.join(format!("{stem}{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!(
        "{stem}.ddn-replace-trash-{}.tmp",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    ))
}

#[cfg(unix)]
fn create_file_symlink(target_path: &Path, link_path: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target_path, link_path)
}

#[cfg(windows)]
fn create_file_symlink(target_path: &Path, link_path: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target_path, link_path)
}

#[cfg(not(any(unix, windows)))]
fn create_file_symlink(_target_path: &Path, _link_path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "symlink creation not supported on this OS",
    ))
}

fn reveal_in_file_browser(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let arg = format!("/select,{}", path.display());
        let status = Command::new("explorer").arg(arg).status()?;
        if status.success() {
            return Ok(());
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("explorer exited with status {status}"),
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open").arg("-R").arg(path).status()?;
        if status.success() {
            return Ok(());
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("open exited with status {status}"),
        ));
    }

    #[cfg(target_os = "linux")]
    {
        if try_spawn_command("nautilus", &[OsStr::new("--select"), path.as_os_str()])? {
            return Ok(());
        }
        if try_spawn_command(
            "nemo",
            &[
                OsStr::new("--no-desktop"),
                OsStr::new("--browser"),
                path.as_os_str(),
            ],
        )? {
            return Ok(());
        }
        if try_spawn_command("dolphin", &[OsStr::new("--select"), path.as_os_str()])? {
            return Ok(());
        }
        if try_spawn_command("thunar", &[path.as_os_str()])? {
            return Ok(());
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        if try_spawn_command("xdg-open", &[parent.as_os_str()])? {
            return Ok(());
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no file manager launcher found (tried nautilus/nemo/dolphin/thunar/xdg-open)",
        ));
    }

    #[allow(unreachable_code)]
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "file-manager reveal is not implemented for this OS",
    ))
}

fn move_path_to_trash(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        if try_status_command("gio", &[OsStr::new("trash"), path.as_os_str()])? {
            return Ok(());
        }
        if try_status_command(
            "kioclient5",
            &[OsStr::new("move"), path.as_os_str(), OsStr::new("trash:/")],
        )? {
            return Ok(());
        }
        if try_status_command(
            "kioclient",
            &[OsStr::new("move"), path.as_os_str(), OsStr::new("trash:/")],
        )? {
            return Ok(());
        }
        if try_status_command("trash-put", &[path.as_os_str()])? {
            return Ok(());
        }
        if try_status_command("gvfs-trash", &[path.as_os_str()])? {
            return Ok(());
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no trash command found (tried gio/kioclient5/kioclient/trash-put/gvfs-trash)",
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let escaped = applescript_escape(path);
        let script = format!("tell application \"Finder\" to delete POSIX file \"{escaped}\"");
        if try_status_command("osascript", &[OsStr::new("-e"), OsStr::new(&script)])? {
            return Ok(());
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "osascript failed to move file to trash",
        ));
    }

    #[cfg(target_os = "windows")]
    {
        let escaped = powershell_single_quote_escape(path);
        let script = format!(
            "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('{escaped}', 'OnlyErrorDialogs', 'SendToRecycleBin')"
        );
        if try_status_command(
            "powershell",
            &[
                OsStr::new("-NoProfile"),
                OsStr::new("-NonInteractive"),
                OsStr::new("-Command"),
                OsStr::new(&script),
            ],
        )? || try_status_command(
            "pwsh",
            &[
                OsStr::new("-NoProfile"),
                OsStr::new("-NonInteractive"),
                OsStr::new("-Command"),
                OsStr::new(&script),
            ],
        )? {
            return Ok(());
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "failed to move file to Recycle Bin via PowerShell",
        ));
    }

    #[allow(unreachable_code)]
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "trash is not implemented for this OS",
    ))
}

fn try_status_command(program: &str, args: &[&OsStr]) -> std::io::Result<bool> {
    match Command::new(program).args(args).status() {
        Ok(status) => Ok(status.success()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

fn try_spawn_command(program: &str, args: &[&OsStr]) -> std::io::Result<bool> {
    match Command::new(program).args(args).spawn() {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

#[cfg(target_os = "macos")]
fn applescript_escape(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(target_os = "windows")]
fn powershell_single_quote_escape(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

struct MatchesTui {
    terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
}

impl MatchesTui {
    fn start() -> std::io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        Ok(Self { terminal })
    }

    fn render(&mut self, state: &MatchesUiState) -> std::io::Result<()> {
        self.terminal.draw(|frame| draw_matches_ui(frame, state))?;
        Ok(())
    }
}

impl Drop for MatchesTui {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = self.terminal.backend_mut().execute(LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn draw_matches_ui(frame: &mut ratatui::Frame<'_>, state: &MatchesUiState) {
    let groups = state.filtered_groups();
    let rows = state.visible_rows();
    let pretty = detect_visual_mode() == VisualMode::Pretty;
    let border_style = if pretty {
        Style::default().fg(Color::Rgb(90, 100, 120))
    } else {
        Style::default()
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(10),
        ])
        .split(frame.area());

    let header = Paragraph::new(format!(
        "dupdupninja matches | mode={} | groups={} | keys: j/k move, Enter expand/open, Shift+Enter ffmpeg, Del trash, Shift+Del delete, r symlink, a/e/s mode, +/- conf, 0 clear, q quit",
        match state.mode {
            MatchMode::All => "all",
            MatchMode::Exact => "exact",
            MatchMode::Similar => "similar",
        },
        groups.len()
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(if pretty { border::ROUNDED } else { border::PLAIN })
            .border_style(border_style)
            .title("Matches"),
    );
    frame.render_widget(header, layout[0]);

    let status = state
        .status_message
        .clone()
        .unwrap_or_else(|| "ready".to_string());
    let threshold_text = Paragraph::new(format!(
        "similarity thresholds: phash<= {}, dhash<= {}, ahash<= {} | min confidence: {:.0}%\nstatus: {}",
        state.thresholds.phash,
        state.thresholds.dhash,
        state.thresholds.ahash,
        state.min_confidence,
        status
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(if pretty {
                border::ROUNDED
            } else {
                border::PLAIN
            })
            .border_style(border_style)
            .title("Config"),
    );
    frame.render_widget(threshold_text, layout[1]);

    let mut list_items = Vec::new();
    for row in &rows {
        match row {
            VisibleRow::Group(gidx) => {
                let g = &groups[*gidx];
                let marker = if state.expanded.contains(gidx) {
                    "[-]"
                } else {
                    "[+]"
                };
                let kind = match g.kind {
                    MatchGroupKind::Exact => "EXACT",
                    MatchGroupKind::Similar => "SIM",
                };
                list_items.push(ListItem::new(format!(
                    "{} {} {} ({}) [{:.2}%]",
                    marker, kind, g.title, g.summary, g.confidence_pct
                )));
            }
            VisibleRow::Entry(gidx, eidx) => {
                let e = &groups[*gidx].entries[*eidx];
                let detail = e.detail.clone().unwrap_or_default();
                if detail.is_empty() {
                    list_items.push(ListItem::new(format!(
                        "    • {}  [{}]",
                        e.path.display(),
                        human_bytes(e.size_bytes)
                    )));
                } else {
                    list_items.push(ListItem::new(format!(
                        "    • {}  [{} | {}]",
                        e.path.display(),
                        human_bytes(e.size_bytes),
                        detail
                    )));
                }
            }
        }
    }
    if list_items.is_empty() {
        list_items.push(ListItem::new("No groups in current mode"));
    }
    let list = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(if pretty {
                    border::ROUNDED
                } else {
                    border::PLAIN
                })
                .border_style(border_style)
                .title("Groups"),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    let mut stateful = ListState::default();
    stateful.select(Some(state.selected.min(rows.len().saturating_sub(1))));
    frame.render_stateful_widget(list, layout[2], &mut stateful);

    let detail_text = match state.selected_row() {
        Some(VisibleRow::Group(gidx)) => {
            let g = &groups[gidx];
            format!(
                "Group: {}\nType: {:?}\nConfidence: {:.2}%\nEntries: {}\nRepresentative size: {}\nTip: Enter expands/collapses groups. On file rows, Enter opens file manager selection.",
                g.title,
                g.kind,
                g.confidence_pct,
                g.entries.len(),
                human_bytes(g.sort_size_bytes),
            )
        }
        Some(VisibleRow::Entry(gidx, eidx)) => {
            let e = &groups[gidx].entries[eidx];
            format_match_entry_details(e, state.show_full_ffmpeg_metadata)
        }
        None => "No selection".to_string(),
    };
    let details = Paragraph::new(detail_text)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(if pretty {
                    border::ROUNDED
                } else {
                    border::PLAIN
                })
                .border_style(border_style)
                .title("Details"),
        );
    frame.render_widget(details, layout[3]);
}

fn format_match_entry_details(entry: &MatchEntry, show_full_ffmpeg: bool) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Path: {}", entry.path.display()));
    lines.push(format!("DB file id: {}", entry.file_id));
    lines.push(format!("Size: {}", human_bytes(entry.size_bytes)));
    let modified = entry
        .modified_at
        .map(format_system_time_utc)
        .unwrap_or_else(|| "n/a".to_string());
    lines.push(format!("Modified (UTC): {modified}"));
    lines.push(format!(
        "Type: {}",
        entry.file_type.as_deref().unwrap_or("unknown")
    ));
    lines.push(format!(
        "Match detail: {}",
        entry
            .detail
            .as_deref()
            .unwrap_or("no additional similarity detail")
    ));
    if let Some(ffmpeg) = entry.ffmpeg_metadata.as_deref() {
        if show_full_ffmpeg {
            lines.push("ffmpeg metadata (full):".to_string());
            lines.push(trim_multiline(ffmpeg, 100, 220));
        } else {
            lines.push("ffmpeg metadata (preview, Shift+Enter for full):".to_string());
            lines.push(trim_multiline(ffmpeg, 14, 180));
        }
    } else {
        lines.push("ffmpeg metadata: n/a".to_string());
    }
    lines.join("\n")
}

fn trim_multiline(input: &str, max_lines: usize, max_line_len: usize) -> String {
    let mut out = Vec::new();
    for (idx, line) in input.lines().enumerate() {
        if idx >= max_lines {
            out.push(format!(
                "... ({} more lines)",
                input.lines().count() - max_lines
            ));
            break;
        }
        if line.chars().count() > max_line_len {
            let shortened: String = line.chars().take(max_line_len).collect();
            out.push(format!("{shortened}..."));
        } else {
            out.push(line.to_string());
        }
    }
    out.join("\n")
}

fn format_system_time_utc(time: SystemTime) -> String {
    let Ok(duration) = time.duration_since(UNIX_EPOCH) else {
        return "before-epoch".to_string();
    };
    let total_secs = duration.as_secs() as i64;
    let days = total_secs.div_euclid(86_400);
    let secs_of_day = total_secs.rem_euclid(86_400);
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year as i32, month as u32, day as u32)
}

fn similarity_scores(
    a: &dupdupninja_core::models::FileListRow,
    b: &dupdupninja_core::models::FileListRow,
) -> SimilarityScores {
    let phash = hash_score(a.phash, b.phash);
    let dhash = hash_score(a.dhash, b.dhash);
    let ahash = hash_score(a.ahash, b.ahash);
    let overall_pct = if let Some(score) = phash {
        score.confidence_pct
    } else {
        let mut sum = 0.0;
        let mut count = 0.0;
        for score in [dhash, ahash].into_iter().flatten() {
            sum += score.confidence_pct;
            count += 1.0;
        }
        if count > 0.0 {
            sum / count
        } else {
            0.0
        }
    };

    SimilarityScores {
        overall_pct,
        phash,
        dhash,
        ahash,
    }
}

fn passes_similarity(scores: SimilarityScores, thresholds: SimilarityThresholds) -> bool {
    if let Some(score) = scores.phash {
        return score.distance <= thresholds.phash;
    }
    if let Some(score) = scores.dhash {
        return score.distance <= thresholds.dhash;
    }
    if let Some(score) = scores.ahash {
        return score.distance <= thresholds.ahash;
    }
    false
}

fn hash_score(a: Option<u64>, b: Option<u64>) -> Option<HashScore> {
    let (Some(a), Some(b)) = (a, b) else {
        return None;
    };
    let dist = hamming_distance(a, b);
    let mut confidence = ((64_u32.saturating_sub(dist)) as f64 / 64.0) * 100.0;
    if confidence >= 100.0 {
        confidence = 99.99;
    }
    Some(HashScore {
        distance: dist,
        confidence_pct: confidence,
    })
}

fn format_hash_score(score: Option<HashScore>) -> String {
    match score {
        Some(s) => format!("{:.1}% (d={})", s.confidence_pct, s.distance),
        None => "n/a".to_string(),
    }
}

fn short_hash_hex(hash: &[u8; 32]) -> String {
    let mut out = String::with_capacity(16);
    for byte in &hash[..8] {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}
