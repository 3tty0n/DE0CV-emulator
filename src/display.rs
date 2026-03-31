use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

/// Render a 7-segment display character from active-low segment bits
/// Segment mapping (gfedcba):
///   bit 0 = a (top)
///   bit 1 = b (top-right)
///   bit 2 = c (bottom-right)
///   bit 3 = d (bottom)
///   bit 4 = e (bottom-left)
///   bit 5 = f (top-left)
///   bit 6 = g (middle)
///
/// Display layout (5 wide x 5 tall):
///  _____
/// |     |
/// |_____|
/// |     |
/// |_____|
pub struct Seg7Widget {
    /// Active-low segment states
    segments: [bool; 7],
}

impl Seg7Widget {
    pub fn new(segments: [bool; 7]) -> Self {
        Self { segments }
    }

    /// Check if segment is ON (active-low: false = on)
    fn is_on(&self, seg: usize) -> bool {
        !self.segments[seg]
    }
}

impl Widget for Seg7Widget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 5 || area.height < 5 {
            return;
        }

        let on_style = Style::default().fg(Color::Red);
        let off_style = Style::default().fg(Color::DarkGray);

        let x = area.x;
        let y = area.y;

        // Segment a (top): row 0, cols 1-3
        let style = if self.is_on(0) { on_style } else { off_style };
        for dx in 1..4 {
            buf.set_string(x + dx, y, "─", style);
        }

        // Segment f (top-left): rows 1-2, col 0
        let style = if self.is_on(5) { on_style } else { off_style };
        buf.set_string(x, y + 1, "│", style);

        // Segment b (top-right): rows 1-2, col 4
        let style = if self.is_on(1) { on_style } else { off_style };
        buf.set_string(x + 4, y + 1, "│", style);

        // Segment g (middle): row 2, cols 1-3
        let style = if self.is_on(6) { on_style } else { off_style };
        for dx in 1..4 {
            buf.set_string(x + dx, y + 2, "─", style);
        }

        // Segment e (bottom-left): rows 3-4, col 0
        let style = if self.is_on(4) { on_style } else { off_style };
        buf.set_string(x, y + 3, "│", style);

        // Segment c (bottom-right): rows 3-4, col 4
        let style = if self.is_on(2) { on_style } else { off_style };
        buf.set_string(x + 4, y + 3, "│", style);

        // Segment d (bottom): row 4, cols 1-3
        let style = if self.is_on(3) { on_style } else { off_style };
        for dx in 1..4 {
            buf.set_string(x + dx, y + 4, "─", style);
        }
    }
}

/// Render an LED indicator
pub struct LedWidget {
    pub on: bool,
    pub label: String,
}

impl LedWidget {
    pub fn new(on: bool, label: String) -> Self {
        Self { on, label }
    }
}

impl Widget for LedWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 2 || area.height < 2 {
            return;
        }

        let style = if self.on {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let symbol = if self.on { "●" } else { "○" };
        buf.set_string(area.x, area.y, symbol, style);
        buf.set_string(
            area.x,
            area.y + 1,
            &self.label,
            Style::default().fg(Color::White),
        );
    }
}
