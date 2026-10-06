//! Panel rendering — three panels, no catalogue, no analysis.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Widget, Wrap},
};

use crate::{
    app::{App, DataMode, EditTarget, Pane, Screen, bits_to_string},
    layout,
    theme::Theme,
    widgets::{self, BitCell},
};

fn field(label: &str, value: &str, theme: &Theme, value_style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<16}"), Style::default().fg(theme.secondary)),
        Span::styled(value.to_string(), value_style),
    ])
}

/// Inputs + transmitted frame.
pub fn render_inputs(area: Rect, app: &App, buf: &mut ratatui::buffer::Buffer, theme: &Theme) {
    let focused = app.focus == Pane::Inputs;
    let badge = if app.editing.is_some() { "TYPING" } else { "i/d=edit" };
    let block = widgets::panel(area, Pane::Inputs.title(), focused, theme, Some((badge, theme.accent)));
    let inner = block.inner(area);
    block.render(area, buf);

    let mut lines: Vec<Line<'static>> = vec![
        Line::from(vec![
            Span::styled("data as [o]  ", Style::default().fg(theme.secondary)),
            Span::styled(
                match app.data_mode {
                    DataMode::Text => "TEXT (type any word)",
                    DataMode::Bits => "BITS (type 0/1)",
                },
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    // Data field.
    if app.editing == Some(EditTarget::Data) {
        let mut spans = vec![Span::styled("data bits       ", Style::default().fg(theme.secondary))];
        for (idx, ch) in app.data_text.chars().enumerate() {
            if idx == app.cursor {
                spans.push(Span::styled(ch.to_string(), Style::default().fg(ratatui::style::Color::Black).bg(theme.accent)));
            } else {
                spans.push(Span::styled(ch.to_string(), Style::default().fg(theme.text).add_modifier(Modifier::BOLD)));
            }
        }
        if app.cursor >= app.data_text.len() {
            spans.push(Span::styled(" ", Style::default().bg(theme.accent)));
        }
        spans.push(Span::styled("  [Esc done]", Style::default().fg(theme.comment)));
        lines.push(Line::from(spans));
    } else if app.data_mode == DataMode::Text {
        lines.push(field("message  [d]", &app.data_text, theme, Style::default().fg(theme.text).add_modifier(Modifier::BOLD)));
        lines.push(field("as bits", &bits_to_string(&app.data), theme, Style::default().fg(theme.comment)));
    } else {
        lines.push(field("data bits [d]", &app.data_text, theme, Style::default().fg(theme.text).add_modifier(Modifier::BOLD)));
    }

    // Generator field.
    if app.editing == Some(EditTarget::Generator) {
        let mut spans = vec![Span::styled("generator  [g]  ", Style::default().fg(theme.secondary))];
        for (idx, ch) in app.generator_text.chars().enumerate() {
            if idx == app.cursor {
                spans.push(Span::styled(ch.to_string(), Style::default().fg(ratatui::style::Color::Black).bg(theme.accent)));
            } else {
                spans.push(Span::styled(ch.to_string(), Style::default().fg(theme.text).add_modifier(Modifier::BOLD)));
            }
        }
        if app.cursor >= app.generator_text.len() {
            spans.push(Span::styled(" ", Style::default().bg(theme.accent)));
        }
        spans.push(Span::styled("  [Esc done]", Style::default().fg(theme.comment)));
        lines.push(Line::from(spans));
    } else {
        lines.push(field("generator  [g]", &app.generator_text, theme, Style::default().fg(theme.text).add_modifier(Modifier::BOLD)));
    }

    if let Some(err) = &app.error {
        lines.push(Line::from(Span::styled(err.clone(), Style::default().fg(theme.red))));
    } else {
        lines.push(field(
            "check bits (n-1)",
            &format!("{}", app.generator.len().saturating_sub(1)),
            theme,
            Style::default().fg(theme.comment),
        ));
        lines.push(field("CRC remainder", &bits_to_string(&app.crc), theme, Style::default().fg(theme.yellow).add_modifier(Modifier::BOLD)));
        lines.push(field("frame=data+CRC", &bits_to_string(&app.frame), theme, Style::default().fg(theme.secondary)));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("transmitted frame", Style::default().fg(theme.secondary))));
        let cells: Vec<BitCell> = app.frame.iter().enumerate().map(|(i, &b)| {
            if i < app.data.len() { BitCell::Data(b) } else { BitCell::Check(b) }
        }).collect();
        for chunk in cells.chunks(28) {
            lines.push(widgets::bit_grid(chunk, None, theme));
        }
    }

    Paragraph::new(lines).render(inner, buf);
}

/// Division tableau with step cursor.
pub fn render_division(area: Rect, app: &App, buf: &mut ratatui::buffer::Buffer, theme: &Theme) {
    let focused = app.focus == Pane::Division;
    let mode = if app.playing { "PLAY" } else { "STEP" };
    let block = widgets::panel(area, Pane::Division.title(), focused, theme, Some((mode, theme.secondary)));
    let inner = block.inner(area);
    block.render(area, buf);

    if app.steps.is_empty() {
        Paragraph::new(Line::from(Span::styled("Fix inputs to see the division.", Style::default().fg(theme.comment))))
            .render(inner, buf);
        return;
    }

    // Classic schoolbook layout: quotient on top, then one staggered
    // subtraction row per step, like decimal long division. "Subtract" here
    // means XOR — in binary arithmetic without carries the two are identical.
    let generator_s = bits_to_string(&app.generator);
    let dividend = bits_to_string(&app.augmented);
    let quotient: String = app.steps.iter().map(|s| if s.xored { '1' } else { '0' }).collect();

    let mut lines: Vec<Line<'static>> = vec![
        Line::from(vec![
            Span::styled("quotient  ", Style::default().fg(theme.secondary)),
            Span::styled(quotient, Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled(format!("divisor {generator_s} | "), Style::default().fg(theme.accent)),
            Span::styled(dividend, Style::default().fg(theme.text)),
        ]),
        Line::from(""),
    ];

    // Collect one 3-line group per subtraction step, then show a window that
    // follows the cursor — otherwise long divisions scroll off the panel with
    // no way to look back.
    let mut groups: Vec<(usize, Vec<Line<'static>>)> = Vec::new();
    for st in app.steps.iter().take(app.step + 1) {
        if !st.xored {
            continue;
        }
        let indent = " ".repeat(8 + st.index);
        let style = if st.index == app.step {
            Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.green)
        };
        groups.push((st.index, vec![
            Line::from(Span::styled(format!("{indent}{generator_s}"), style)),
            Line::from(Span::styled(format!("{indent}{}", "─".repeat(generator_s.len())), Style::default().fg(theme.muted))),
            Line::from(Span::styled(format!("{indent}{}", bits_to_string(&st.bits[st.index..])), style)),
        ]));
    }

    let reserved = 5; // header (3) + footer (2)
    let rows_fit = (inner.height as usize).saturating_sub(reserved) / 3;
    let rows_fit = rows_fit.max(1);
    // Manual scroll-back, clamped; stepping resets it to follow the cursor.
    let total = groups.len();
    let max_back = total.saturating_sub(rows_fit);
    let back = app.div_scroll.min(max_back);
    let end = total.saturating_sub(back);
    let start = end.saturating_sub(rows_fit);
    for (_, g) in groups.iter().skip(start).take(end - start) {
        lines.extend(g.clone());
    }
    if back > 0 {
        lines.push(Line::from(Span::styled(
            format!("…{back} earlier row(s) — PgDn to follow"),
            Style::default().fg(theme.comment),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(format!("step {}/{}  ", app.step + 1, app.steps.len()), Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
        Span::styled("space=play/pause  ←/→=step  PgUp/PgDn=scroll", Style::default().fg(theme.comment)),
    ]));
    Paragraph::new(lines).render(inner, buf);
}

/// Channel + receiver.
pub fn render_receiver(area: Rect, app: &App, buf: &mut ratatui::buffer::Buffer, theme: &Theme) {
    let focused = app.focus == Pane::Receiver;
    let (label, color) = if app.frame.is_empty() {
        ("—", theme.muted)
    } else if app.is_valid() {
        ("VALID", theme.green)
    } else {
        ("ERROR DETECTED", theme.red)
    };
    let block = widgets::panel(area, Pane::Receiver.title(), focused, theme, Some((label, color)));
    let inner = block.inner(area);
    block.render(area, buf);

    if app.frame.is_empty() {
        Paragraph::new(Line::from(Span::styled("Nothing to receive yet.", Style::default().fg(theme.comment)))).render(inner, buf);
        return;
    }

    let mut lines = vec![
        field(
            "flipped bits",
            &if app.damaged.is_empty() { "none".to_string() } else { format!("{:?}", app.damaged) },
            theme,
            if app.damaged.is_empty() { Style::default().fg(theme.comment) } else { Style::default().fg(theme.red) },
        ),
        field("receiver remainder", &bits_to_string(&app.remainder()), theme,
            if app.is_valid() { Style::default().fg(theme.green) } else { Style::default().fg(theme.red).add_modifier(Modifier::BOLD) }),
        Line::from(""),
        Line::from(Span::styled("received frame (click-equivalent: e flips, x resets)", Style::default().fg(theme.secondary))),
    ];
    let cells: Vec<BitCell> = app.received.iter().enumerate().map(|(i, &b)| {
        let was = app.frame.get(i).copied().unwrap_or(0);
        if b != was { BitCell::Damaged { was, now: b } }
        else if i < app.data.len() { BitCell::Data(b) } else { BitCell::Check(b) }
    }).collect();
    for chunk in cells.chunks(28) {
        lines.push(widgets::bit_grid(chunk, None, theme));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        if app.is_valid() { "VALID — remainder all zero, no error." } else { "ERROR — remainder non-zero, a bit was changed." },
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )));
    Paragraph::new(lines).wrap(Wrap { trim: true }).render(inner, buf);
}

pub fn render_help(area: Rect, buf: &mut ratatui::buffer::Buffer, theme: &Theme) {
    let popup = layout::popup(60, 60, area);
    Clear.render(popup, buf);
    let block = widgets::panel(popup, "KEYS", true, theme, None);
    let inner = block.inner(popup);
    block.render(popup, buf);
    let rows: &[(&str, &str)] = &[
        ("tab", "next panel"),
        ("d / g", "edit message / generator"),
        ("o", "toggle text / bits input"),
        ("t", "next theme"),
        ("esc / enter", "finish typing"),
        ("space", "play or pause the division (auto-step)"),
        ("← / →", "one division step"),
        ("PgUp / PgDn", "scroll division history"),
        ("e", "flip one random bit (channel error)"),
        ("x", "reset channel (no error)"),
        ("?", "this help"),
        ("q", "quit"),
    ];
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (k, d) in rows {
        lines.push(Line::from(vec![
            Span::styled(format!("  {k:<10}"), Style::default().fg(theme.yellow)),
            Span::styled(*d, Style::default().fg(theme.text)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Play = auto-step through the division so you can watch each XOR.", Style::default().fg(theme.comment))));
    Paragraph::new(lines).render(inner, buf);
}

pub fn render_footer(app: &App, theme: &Theme) -> Line<'static> {
    let hints: &[(&str, &str)] = if app.editing.is_some() {
        &[("esc", "done"), ("type", "live input")]
    } else {
        match app.focus {
            Pane::Inputs => &[("d/g", "edit"), ("o", "text/bits"), ("t", "theme")],
            Pane::Division => &[("space", "play"), ("←→", "step"), ("t", "theme")],
            Pane::Receiver => &[("e", "error"), ("x", "reset"), ("t", "theme")],
        }
    };
    let _ = theme;
    let mut spans = vec![Span::styled(
        " CRC ",
        Style::default().fg(ratatui::style::Color::Black).bg(theme.accent).add_modifier(Modifier::BOLD),
    )];
    for (k, label) in hints {
        spans.push(Span::styled(format!(" {k} ",), Style::default().fg(ratatui::style::Color::Black).bg(theme.secondary).add_modifier(Modifier::BOLD)));
        spans.push(Span::styled(format!(" {label}  "), Style::default().fg(theme.comment)));
    }
    if app.screen == Screen::Help {
        spans.push(Span::styled(" esc close ", Style::default().fg(theme.comment)));
    }
    Line::from(spans)
}

pub fn render_header(area: Rect, app: &App, buf: &mut ratatui::buffer::Buffer, theme: &Theme) {
    let block = widgets::panel(area, "CRC LAB", false, theme, None);
    let inner = block.inner(area);
    block.render(area, buf);
    let (status, color) = if app.frame.is_empty() {
        ("waiting for valid inputs", theme.muted)
    } else if app.is_valid() {
        ("no error", theme.green)
    } else {
        ("error detected", theme.red)
    };
    Paragraph::new(Line::from(vec![
        Span::styled("Cyclic Redundancy Check ", Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::styled("data + generator → division → verify  ", Style::default().fg(theme.comment)),
        Span::styled(status, Style::default().fg(color).add_modifier(Modifier::BOLD)),
    ]))
    .render(inner, buf);
}
