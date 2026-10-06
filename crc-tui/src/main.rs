//! CRC Lab (TUI): textbook CRC. Single theme, three panels.

mod app;
mod layout;
mod panels;
mod theme;
mod widgets;

use std::{
    io::{self, Stdout, stdout},
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, widgets::Widget};

use app::{App, DataMode, EditTarget, Pane, Screen};
use theme::THEMES;

const TICK: Duration = Duration::from_millis(400);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
        previous_hook(info);
    }));

    let mut app = App::default();
    let outcome = run(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = outcome {
        eprintln!("crc-tui: {e}");
        return Err(e);
    }
    Ok(())
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;
        if event::poll(TICK)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key(app, key);
                }
            }
        }
        // Same tick drives Play: auto-advance one division step.
        app.tick();
        if app.should_quit {
            return Ok(());
        }
    }
}

fn draw(frame: &mut ratatui::Frame<'_>, app: &mut App) {
    let theme = &THEMES[app.theme_index % THEMES.len()];
    let area = frame.area();
    let (header, body, footer) = layout::header_body_footer(area);
    panels::render_header(header, app, frame.buffer_mut(), theme);

    let (left, right) = layout::columns(body);
    let (inputs, receiver) = layout::left_stack(left);
    panels::render_inputs(inputs, app, frame.buffer_mut(), theme);
    panels::render_receiver(receiver, app, frame.buffer_mut(), theme);
    panels::render_division(right, app, frame.buffer_mut(), theme);

    if app.screen == Screen::Help {
        panels::render_help(area, frame.buffer_mut(), theme);
    }
    frame.render_widget(
        ratatui::widgets::Paragraph::new(panels::render_footer(app, theme)),
        footer,
    );
}

fn handle_key(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }
    if app.screen == Screen::Help {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter) {
            app.screen = Screen::Lab;
        }
        return;
    }

    // Typing mode: only 0/1, backspace, arrows, esc/enter.
    if let Some(target) = app.editing {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => app.exit_editing(),
            KeyCode::Backspace => {
                let text = match target {
                    EditTarget::Data => &mut app.data_text,
                    EditTarget::Generator => &mut app.generator_text,
                };
                if app.cursor > 0 && !text.is_empty() {
                    let pos = (app.cursor - 1).min(text.len().saturating_sub(1));
                    text.remove(pos);
                    app.cursor = app.cursor.saturating_sub(1);
                }
            }
            KeyCode::Left => app.cursor = app.cursor.saturating_sub(1),
            KeyCode::Right => {
                let len = match target {
                    EditTarget::Data => app.data_text.len(),
                    EditTarget::Generator => app.generator_text.len(),
                };
                app.cursor = (app.cursor + 1).min(len);
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let is_data = target == EditTarget::Data;
                let text_mode = is_data && app.data_mode == DataMode::Text;
                if text_mode {
                    // Any printable character: this is the "type a name" path.
                    if (ch.is_ascii_graphic() || ch == ' ') && app.data_text.len() < 12 {
                        let pos = app.cursor.min(app.data_text.len());
                        app.data_text.insert(pos, ch);
                        app.cursor += 1;
                    }
                } else {
                    // Bits mode + generator: binary only.
                    if (ch == '0' || ch == '1') {
                        if is_data {
                            if app.data_text.len() < 64 {
                                let pos = app.cursor.min(app.data_text.len());
                                app.data_text.insert(pos, ch);
                                app.cursor += 1;
                            }
                        } else if app.generator_text.len() < 17 {
                            let pos = app.cursor.min(app.generator_text.len());
                            app.generator_text.insert(pos, ch);
                            app.cursor += 1;
                        }
                    }
                }
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => app.screen = Screen::Help,
        KeyCode::Tab => {
            let order = Pane::ORDER;
            let cur = order.iter().position(|p| *p == app.focus).unwrap_or(0);
            app.focus = order[(cur + 1) % order.len()];
        }
        KeyCode::BackTab => {
            let order = Pane::ORDER;
            let cur = order.iter().position(|p| *p == app.focus).unwrap_or(0);
            app.focus = order[(cur + order.len() - 1) % order.len()];
        }
        // Live inputs — this is what proves it is not a fixed demo.
        KeyCode::Char('d') | KeyCode::Char('D') => {
            app.focus = Pane::Inputs;
            app.enter_editing(EditTarget::Data);
        }
        KeyCode::Char('g') | KeyCode::Char('G') => {
            app.focus = Pane::Inputs;
            app.enter_editing(EditTarget::Generator);
        }
        // Play/pause = auto-step through the division so the class can watch.
        KeyCode::Char(' ') => {
            if !app.steps.is_empty() {
                app.playing = !app.playing;
            }
        }
        KeyCode::Left => {
            app.playing = false;
            app.step = app.step.saturating_sub(1);
            app.div_scroll = 0;
        }
        KeyCode::Right => {
            app.playing = false;
            if !app.steps.is_empty() {
                app.step = (app.step + 1).min(app.steps.len() - 1);
            }
            app.div_scroll = 0;
        }
        KeyCode::PageUp => {
            app.div_scroll += 1;
        }
        KeyCode::PageDown => {
            app.div_scroll = app.div_scroll.saturating_sub(1);
        }
        KeyCode::Char('e') | KeyCode::Char('E') => app.inject_random(),
        KeyCode::Char('x') | KeyCode::Char('X') => app.clear_errors(),
        KeyCode::Char('o') | KeyCode::Char('O') => {
            app.data_mode = match app.data_mode {
                DataMode::Text => {
                    app.data_text = "1011001".to_string();
                    DataMode::Bits
                }
                DataMode::Bits => {
                    app.data_text = "HELLO".to_string();
                    DataMode::Text
                }
            };
            app.rebuild();
        }
        KeyCode::Char('t') | KeyCode::Char('T') => {
            app.theme_index = (app.theme_index + 1) % THEMES.len();
        }
        _ => {}
    }
}
