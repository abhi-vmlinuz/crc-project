//! Rendering helpers: bit grids, the long-division tableau, and a bar chart.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders},
};

use crate::theme::Theme;

/// Draw a framed panel with a title and an optional right-aligned badge.
#[must_use]
pub fn panel(
    area: Rect,
    title: &str,
    focused: bool,
    theme: &Theme,
    badge: Option<(&str, Color)>,
) -> Block<'static> {
    let border_color = if focused { theme.accent } else { theme.muted };
    let title_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.comment)
    };

    let mut spans = vec![Span::styled(format!(" {title} "), title_style)];
    if let Some((text, color)) = badge {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("[{text}]"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }

    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused {
            BorderType::Rounded
        } else {
            BorderType::Plain
        })
        .border_style(Style::default().fg(border_color))
        .title(Line::from(spans))
}

/// A cell in a bit grid, carrying the state the renderer needs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BitCell {
    /// A transmitted data bit, untouched.
    Data(u8),
    /// A check bit appended by the CRC.
    Check(u8),
    /// A bit the channel flipped: shows the original value underneath.
    Damaged { was: u8, now: u8 },
    /// A bit the channel flipped back to its original value.
    Repaired { was: u8, now: u8 },
}

/// Render a bit grid as a row of styled cells.
pub fn bit_grid(cells: &[BitCell], cursor: Option<usize>, theme: &Theme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(cells.len());
    for (i, cell) in cells.iter().enumerate() {
        let selected = cursor == Some(i);
        let (text, style) = match cell {
            BitCell::Data(bit) => (
                if *bit == 1 { "1" } else { "0" },
                if *bit == 1 {
                    Style::default()
                        .fg(theme.text)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.comment)
                },
            ),
            BitCell::Check(bit) => (
                if *bit == 1 { "1" } else { "0" },
                if *bit == 1 {
                    Style::default()
                        .fg(theme.secondary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(theme.muted)
                        .add_modifier(Modifier::DIM)
                },
            ),
            // A damaged cell shows the received value; the original is implied
            // by the strikethrough styling and the red colour.
            BitCell::Damaged { now, .. } => (
                if *now == 1 { "1" } else { "0" },
                Style::default().fg(theme.red).add_modifier(Modifier::BOLD),
            ),
            BitCell::Repaired { now, .. } => (
                if *now == 1 { "1" } else { "0" },
                Style::default().fg(theme.green).add_modifier(Modifier::BOLD),
            ),
        };
        let style = if selected {
            style.bg(theme.accent).fg(Color::Black)
        } else {
            style
        };
        // A space between cells is what makes a 200-bit frame readable; without
        // it the row is an undifferentiated smear.
        spans.push(Span::raw(" "));
        spans.push(Span::styled(text.to_string(), style));
    }
    Line::from(spans)
}

/// One row of the long-division tableau.
#[derive(Clone, Debug)]
pub struct DivisionRow {
    pub label: String,
    pub bits: String,
    pub xored: bool,
    pub active: bool,
}

/// Render the classic dividend/divisor tableau.
///
/// The visual grammar is the one from the course notes: the dividend above the
/// bar, the generator below it, then one row per subtraction.
pub fn division_tableau(
    area: Rect,
    generator: &str,
    dividend: &str,
    rows: &[DivisionRow],
    theme: &Theme,
) -> Vec<Line<'static>> {
    let width = area.width.saturating_sub(2) as usize;
    let mut lines = Vec::new();

    // Dividend, right-aligned above the operator.
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(width.saturating_sub(dividend.len()))),
        Span::styled(dividend.to_string(), Style::default().fg(theme.text)),
    ]));

    // The bar and the generator beneath it.
    lines.push(Line::from(Span::styled(
        "\u{2500}".repeat(generator.len().min(width)),
        Style::default().fg(theme.muted),
    )));
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(width.saturating_sub(generator.len()))),
        Span::styled(
            generator.to_string(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        format!("{} \u{2295}", " ".repeat(width.saturating_sub(2))),
        Style::default().fg(theme.muted),
    )));

    for row in rows {
        let style = if row.active {
            Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)
        } else if row.xored {
            Style::default().fg(theme.green)
        } else {
            Style::default().fg(theme.comment)
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:>3} ", row.label),
                Style::default().fg(theme.muted),
            ),
            Span::styled(row.bits.clone(), style),
        ]));
    }

    lines
}

/// Horizontal bar, used for detection rates in the analysis panel.
pub fn bar(fraction: f64, width: usize, filled: Color, empty: Color) -> Span<'static> {
    let clamped = fraction.clamp(0.0, 1.0);
    let on = (clamped * width as f64).round() as usize;
    let mut text = String::new();
    for _ in 0..on.min(width) {
        text.push('\u{2588}');
    }
    for _ in on.min(width)..width {
        text.push('\u{2591}');
    }
    Span::styled(text, Style::default().fg(if on > 0 { filled } else { empty }))
}

/// A key hint: the key itself, then its label, styled consistently.
#[must_use]
pub fn key(key: &str, label: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {key} "),
            Style::default()
                .fg(theme.bg.unwrap_or(Color::Black))
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {label}   "), Style::default().fg(theme.comment)),
    ])
}
