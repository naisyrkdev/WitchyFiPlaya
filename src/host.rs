use egui::{Context, Rect, ViewportCommand, pos2, vec2};
use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SNAP_EVERY: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Placement {
    Fullscreen,
    Window(Rect),
    Free,
}

impl Placement {
    fn parse(words: &str) -> Option<Self> {
        let mut words = words.split_whitespace();
        match words.next()? {
            "fullscreen" => Some(Placement::Fullscreen),
            "free" => Some(Placement::Free),
            "window" => {
                let n: Vec<f32> = words.map(str::parse).collect::<Result<_, _>>().ok()?;
                let [x, y, w, h] = n[..] else { return None };
                (w > 0.0 && h > 0.0).then(|| Placement::Window(Rect::from_min_size(pos2(x, y), vec2(w, h))))
            }
            _ => None,
        }
    }
}

pub struct Host {
    pub inside_yasaff: bool,
    pub placement: Placement,
    shows: Option<mpsc::Receiver<Placement>>,
    last_snap: Instant,
}

impl Host {
    pub fn from_env() -> Self {
        let placement = if std::env::var_os("YASAFF_FULLSCREEN").is_some() || std::env::args().any(|a| a == "--fullscreen") {
            Placement::Fullscreen
        } else {
            std::env::var("YASAFF_WINDOW").ok().and_then(|w| Placement::parse(&format!("window {w}"))).unwrap_or(Placement::Free)
        };
        Host { inside_yasaff: std::env::var_os("YASAFF").is_some(), placement, shows: None, last_snap: Instant::now() }
    }

    pub fn viewport(&self, builder: egui::ViewportBuilder) -> egui::ViewportBuilder {
        match self.placement {
            Placement::Fullscreen => builder.with_fullscreen(true).with_decorations(false),
            Placement::Window(r) => builder
                .with_decorations(false)
                .with_resizable(false)
                .with_position(r.min)
                .with_inner_size(r.size()),
            Placement::Free => builder,
        }
    }

    pub fn listen(&mut self, ctx: &Context) {
        if !self.inside_yasaff {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                let Some(rest) = line.strip_prefix("show") else { continue };
                let placement = if rest.trim().is_empty() { Some(Placement::Free) } else { Placement::parse(rest) };
                if let Some(placement) = placement {
                    let _ = tx.send(placement);
                    ctx.request_repaint();
                }
            }
        });
        self.shows = Some(rx);
    }

    pub fn minimize(&self, ctx: &Context) {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
        if self.inside_yasaff {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "yasaff:home").and_then(|()| out.flush());
        }
    }

    pub fn update(&mut self, ctx: &Context) {
        let latest = self.shows.as_ref().and_then(|rx| rx.try_iter().last());
        if let Some(placement) = latest {
            if placement != Placement::Free {
                self.placement = placement;
            }
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.placement == Placement::Fullscreen));
            if let Placement::Window(r) = self.placement {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(r.size()));
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(r.min));
            }
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            self.last_snap = Instant::now();
            return;
        }

        let Placement::Window(target) = self.placement else { return };
        let (minimized, outer) = ctx.input(|i| (i.viewport().minimized, i.viewport().outer_rect));
        let Some(outer) = outer.filter(|_| minimized != Some(true)) else { return };
        if self.last_snap.elapsed() < SNAP_EVERY {
            return;
        }
        if outer.min.distance(target.min) > 1.0 {
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(target.min));
            self.last_snap = Instant::now();
        }
        if (outer.size() - target.size()).length() > 1.0 {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(target.size()));
            self.last_snap = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_placements() {
        assert_eq!(Placement::parse(" fullscreen"), Some(Placement::Fullscreen));
        assert_eq!(Placement::parse("free"), Some(Placement::Free));
        assert_eq!(
            Placement::parse("window 10 20.5 1280 720"),
            Some(Placement::Window(Rect::from_min_size(pos2(10.0, 20.5), vec2(1280.0, 720.0))))
        );
        assert_eq!(Placement::parse("window 10 20 1280"), None);
        assert_eq!(Placement::parse("window 0 0 0 720"), None);
    }
}
