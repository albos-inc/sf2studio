//! An ABX blind test: is the hidden X lane A or lane B?

use eframe::egui::{self, Color32, RichText, Vec2};

/// What to hear.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Listen {
    A,
    B,
    X,
}

pub struct Abx {
    pub open: bool,
    /// The two lanes compared.
    pub a: usize,
    pub b: usize,
    x_is_a: bool,
    /// Whether each answer was right.
    answers: Vec<bool>,
    last: Option<bool>,
    seed: u64,
}

impl Abx {
    pub fn new() -> Abx {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9e37_79b9_7f4a_7c15, |d| d.as_nanos() as u64)
            | 1;
        let mut abx = Abx { open: false, a: 0, b: 1, x_is_a: true, answers: Vec::new(), last: None, seed };
        abx.next_trial();
        abx
    }

    fn random_bit(&mut self) -> bool {
        // xorshift64
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed & 1 == 1
    }

    fn next_trial(&mut self) {
        self.x_is_a = self.random_bit();
    }

    fn reset(&mut self) {
        self.answers.clear();
        self.last = None;
        self.next_trial();
    }

    /// Some answers already given, for the guide's screenshots.
    #[cfg(feature = "capture")]
    pub fn demo(&mut self) {
        self.answers = vec![true, true, false, true, true, true, true, true, true, true];
        self.last = Some(true);
    }

    /// The lane to hear for a listen button.
    pub fn lane(&self, listen: Listen) -> usize {
        match listen {
            Listen::A => self.a,
            Listen::B => self.b,
            Listen::X if self.x_is_a => self.a,
            Listen::X => self.b,
        }
    }

    /// The window. Returns the lane to hear when a listen button is pressed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        japanese: bool,
        lane_count: usize,
        letter: impl Fn(usize) -> char,
    ) -> Option<usize> {
        let t = |en: &'static str, ja: &'static str| if japanese { ja } else { en };
        let mut open = self.open;
        let mut hear = None;
        let window = egui::Window::new(t("Blind test (ABX)", "ブラインドテスト（ABX）"))
            .open(&mut open)
            .resizable(false)
            .min_width(340.0)
            .show(ctx, |ui| {
                ui.label(t(
                    "X is A or B at random. Listen to all three, then say which X is.",
                    "X は A か B のどちらかです（毎回ランダム）。3 つを聞き比べて、X がどちらかを答えてください。",
                ));
                ui.add_space(6.0);
                let before = (self.a, self.b);
                ui.horizontal(|ui| {
                    ui.label(t("Compare lanes", "比べるレーン"));
                    for (id, value) in [("abx-a", &mut self.a), ("abx-b", &mut self.b)] {
                        egui::ComboBox::from_id_salt(id).width(50.0).selected_text(letter(*value).to_string()).show_ui(
                            ui,
                            |ui| {
                                for lane in 0..lane_count {
                                    ui.selectable_value(value, lane, letter(lane).to_string());
                                }
                            },
                        );
                    }
                });
                if (self.a, self.b) != before {
                    self.reset();
                }
                if self.a == self.b {
                    ui.colored_label(
                        Color32::from_rgb(240, 190, 90),
                        t("Choose two different lanes.", "違うレーンを 2 つ選んでください。"),
                    );
                    return;
                }
                ui.add_space(6.0);
                let big = Vec2::new(96.0, 36.0);
                ui.horizontal(|ui| {
                    for (listen, label) in [(Listen::A, "A"), (Listen::B, "B"), (Listen::X, "X")] {
                        let text = format!("🔊 {label}");
                        if ui.add(egui::Button::new(RichText::new(text).size(18.0)).min_size(big)).clicked() {
                            hear = Some(self.lane(listen));
                        }
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(t("X is", "X は"));
                    for (is_a, label) in [(true, "A"), (false, "B")] {
                        if ui
                            .add(egui::Button::new(RichText::new(label).size(16.0)).min_size(Vec2::new(60.0, 30.0)))
                            .clicked()
                        {
                            let right = is_a == self.x_is_a;
                            self.answers.push(right);
                            self.last = Some(right);
                            self.next_trial();
                        }
                    }
                });
                if let Some(right) = self.last {
                    ui.colored_label(
                        if right { Color32::from_rgb(110, 210, 120) } else { Color32::from_rgb(240, 110, 110) },
                        if right {
                            t("Right — next X is ready.", "正解 — 次の X を用意しました。")
                        } else {
                            t("Wrong — next X is ready.", "不正解 — 次の X を用意しました。")
                        },
                    );
                }
                ui.separator();
                let n = self.answers.len();
                let k = self.answers.iter().filter(|&&r| r).count();
                let p = chance(k, n);
                ui.label(format!(
                    "{} {k} / {n}   {} {:.1}%",
                    t("Right", "正解"),
                    t("chance of guessing this well", "当てずっぽうでこうなる確率"),
                    p * 100.0
                ));
                if n >= 8 {
                    ui.label(if p < 0.05 {
                        t("You can tell them apart.", "聞き分けられています。")
                    } else {
                        t("Not distinguishable yet.", "まだ聞き分けられているとは言えません。")
                    });
                } else {
                    ui.weak(t("Answer at least 8 times for a meaningful result.", "8 回以上答えると判定できます。"));
                }
                if ui.button(t("Start over", "最初から")).clicked() {
                    self.reset();
                }
            });
        if let Some(window) = window {
            crate::app::mark(ctx, "abx-window", 0, window.response.rect);
        }
        self.open = open;
        hear
    }
}

/// The chance of `k` or more right out of `n` by guessing.
fn chance(k: usize, n: usize) -> f64 {
    if n == 0 {
        return 1.0;
    }
    let mut total = 0.0;
    for i in k..=n {
        total += binomial(n, i);
    }
    total / 2f64.powi(n as i32)
}

fn binomial(n: usize, k: usize) -> f64 {
    let k = k.min(n - k);
    (0..k).fold(1.0, |acc, i| acc * (n - i) as f64 / (i + 1) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chance_of_guessing() {
        assert_eq!(chance(0, 10), 1.0);
        assert!((chance(10, 10) - 1.0 / 1024.0).abs() < 1e-12);
        // 9 or more of 10: 11/1024.
        assert!((chance(9, 10) - 11.0 / 1024.0).abs() < 1e-12);
    }
}
