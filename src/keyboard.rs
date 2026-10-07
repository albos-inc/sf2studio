//! An 88-key piano keyboard. Where a key is pressed sets the velocity:
//! soft at the back (top), loud at the front (bottom).

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use crate::patterns::key_name;

pub const LOWEST: u8 = 21; // A0
pub const HIGHEST: u8 = 108; // C8

pub enum KeyEvent {
    On { key: u8, velocity: u8 },
    Off { key: u8 },
}

#[derive(Default)]
pub struct Keyboard {
    held: Option<u8>,
}

fn is_black(key: u8) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10)
}

/// Velocity for a press at `y` in a key spanning `top..bottom`.
fn velocity_at(y: f32, top: f32, bottom: f32) -> u8 {
    let t = ((y - top) / (bottom - top).max(1.0)).clamp(0.0, 1.0);
    (1.0 + t * 126.0).round() as u8
}

impl Keyboard {
    /// Draws the keyboard and reports presses. `fixed_velocity` overrides the
    /// pressed position; `sounding` lists keys to light (e.g. from MIDI);
    /// `last` marks the last note played with its velocity.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        fixed_velocity: Option<u8>,
        sounding: &[u8],
        last: Option<(u8, u8)>,
        accent: Color32,
    ) -> Vec<KeyEvent> {
        let size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let white_keys: Vec<u8> = (LOWEST..=HIGHEST).filter(|&k| !is_black(k)).collect();
        let white_width = rect.width() / white_keys.len() as f32;
        let black_height = rect.height() * 0.62;

        let mut white_rects = Vec::with_capacity(white_keys.len());
        for (i, &key) in white_keys.iter().enumerate() {
            let x = rect.left() + i as f32 * white_width;
            white_rects
                .push((key, Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(white_width, rect.height()))));
        }
        let mut black_rects = Vec::new();
        for (i, &key) in white_keys.iter().enumerate() {
            let black = key + 1;
            if black <= HIGHEST && is_black(black) {
                let x = rect.left() + (i as f32 + 1.0) * white_width - white_width * 0.3;
                black_rects.push((
                    black,
                    Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(white_width * 0.6, black_height)),
                ));
            }
        }

        let key_at = |pos: Pos2| -> Option<(u8, Rect)> {
            black_rects.iter().chain(white_rects.iter()).find(|(_, r)| r.contains(pos)).map(|&(k, r)| (k, r))
        };
        let hover = response.hover_pos().and_then(|p| key_at(p).map(|(k, r)| (k, r, p)));

        // Presses: a held key follows the pointer (glissando).
        let mut events = Vec::new();
        let down = response.is_pointer_button_down_on();
        let pointer = ui.input(|i| i.pointer.interact_pos());
        match (down, pointer.and_then(key_at)) {
            (true, Some((key, key_rect))) => {
                if self.held != Some(key) {
                    if let Some(old) = self.held.take() {
                        events.push(KeyEvent::Off { key: old });
                    }
                    let y = pointer.map_or(key_rect.center().y, |p| p.y);
                    let velocity = fixed_velocity.unwrap_or_else(|| velocity_at(y, key_rect.top(), key_rect.bottom()));
                    events.push(KeyEvent::On { key, velocity });
                    self.held = Some(key);
                }
            }
            _ => {
                if let Some(old) = self.held.take() {
                    events.push(KeyEvent::Off { key: old });
                }
            }
        }

        let lit = |key: u8| self.held == Some(key) || sounding.contains(&key);
        for &(key, r) in &white_rects {
            let fill = if lit(key) { accent } else { Color32::from_gray(235) };
            painter.rect_filled(r.shrink2(Vec2::new(0.5, 0.0)), 2.0, fill);
            if key % 12 == 0 {
                painter.text(
                    Pos2::new(r.center().x, r.bottom() - 4.0),
                    Align2::CENTER_BOTTOM,
                    key_name(key),
                    FontId::proportional((white_width * 0.45).clamp(7.0, 12.0)),
                    Color32::from_gray(90),
                );
            }
        }
        for &(key, r) in &black_rects {
            let fill = if lit(key) { accent.gamma_multiply(0.8) } else { Color32::from_gray(25) };
            painter.rect_filled(r, 2.0, fill);
        }

        // The last note: a bar on its key as high as its velocity.
        if let Some((key, velocity)) = last {
            if let Some(&(_, r)) = black_rects.iter().chain(white_rects.iter()).find(|(k, _)| *k == key) {
                let height = r.height() * velocity as f32 / 127.0;
                let bar = Rect::from_min_max(
                    Pos2::new(r.left() + 1.0, r.bottom() - height),
                    Pos2::new(r.left() + 4.0, r.bottom()),
                );
                painter.rect_filled(bar, 1.0, Color32::from_rgb(255, 200, 60));
            }
        }

        // Where the pointer is: key, and the velocity a press there gives.
        if let Some((key, r, p)) = hover {
            let velocity = fixed_velocity.unwrap_or_else(|| velocity_at(p.y, r.top(), r.bottom()));
            painter.line_segment([Pos2::new(r.left(), p.y), Pos2::new(r.right(), p.y)], Stroke::new(1.5, accent));
            let label = format!("{}  vel {velocity}", key_name(key));
            let at = Pos2::new((p.x + 10.0).min(rect.right() - 90.0), (p.y - 18.0).max(rect.top() + 2.0));
            let galley = painter.layout_no_wrap(label, FontId::proportional(13.0), Color32::WHITE);
            painter.rect_filled(
                Rect::from_min_size(at, galley.size()).expand(3.0),
                3.0,
                Color32::from_black_alpha(200),
            );
            painter.galley(at, galley, Color32::WHITE);
        }
        events
    }
}
