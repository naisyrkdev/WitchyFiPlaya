#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod audio;
mod background;
mod config;
mod dance;
mod host;
mod music;
mod pad;
mod prompts;

use audio::Audio;
use background::Background;
use config::{Config, Repeat};
use dance::Dancer;
use host::Host;
use egui::{Align2, Color32, Context, FontId, Id, Painter, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use gilrs::Button;
use music::Track;
use pad::{Command, Pad};
use prompts::{Layout, Prompts};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const ACCENT: Color32 = Color32::from_rgb(0x2f, 0x7d, 0xf6);
const TITLE: Color32 = Color32::from_rgb(0x9b, 0x4d, 0xe0);
const PANEL: Color32 = Color32::from_rgb(14, 20, 36);
const PAD_A: Color32 = Color32::from_rgb(0x5c, 0xb8, 0x3c);
const PAD_B: Color32 = Color32::from_rgb(0xd9, 0x3a, 0x3a);
const SEEK_STEP: f32 = 10.0;
const VOLUME_STEP: f32 = 0.05;
const RESTART_AFTER: Duration = Duration::from_secs(3);
const TOAST_TIME: Duration = Duration::from_secs(3);

fn install_fonts(ctx: &Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "eagle_lake".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/EagleLake-Regular.ttf"))),
    );
    fonts.families.insert(title_family(), vec!["eagle_lake".to_owned()]);
    ctx.set_fonts(fonts);
}

fn title_family() -> egui::FontFamily {
    egui::FontFamily::Name("eagle_lake".into())
}

fn main() -> eframe::Result {
    let host = Host::from_env();
    let mut builder = egui::ViewportBuilder::default()
        .with_title("WitchyFiPlaya")
        .with_app_id("witchy-fi-playa");
    match eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        Ok(icon) => builder = builder.with_icon(icon),
        Err(e) => eprintln!("could not load the window icon: {e}"),
    }
    let options = eframe::NativeOptions {
        viewport: host.viewport(
            builder
                .with_inner_size([1280.0, 720.0])
                .with_min_inner_size([640.0, 360.0]),
        ),
        ..Default::default()
    };
    eframe::run_native("WitchyFiPlaya", options, Box::new(|cc| Ok(Box::new(App::new(cc, host)))))
}

enum Mode {
    Library,
    Browse { dir: PathBuf, entries: Vec<PathBuf>, cursor: usize, pick: Pick },
    Settings { cursor: usize },
}

#[derive(Clone, Copy, PartialEq)]
enum Pick {
    Music { from_settings: bool },
    Background,
}

const SETTINGS_ROWS: usize = 4;
const BACKGROUND_ROW: usize = 1;

struct App {
    config: Config,
    audio: Result<Audio, String>,
    tracks: Vec<Track>,
    scanning: Option<mpsc::Receiver<Vec<Track>>>,
    cursor: usize,
    current: Option<usize>,
    history: Vec<usize>,
    dancer: Dancer,
    background: Background,
    mode: Mode,
    pad: Pad,
    host: Host,
    toast: Option<(String, Instant)>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, mut host: Host) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let config = Config::load();
        let audio = Audio::open(config.volume).map_err(|e| format!("{e:#}"));
        host.listen(&cc.egui_ctx);
        let (dancer, warnings) = Dancer::load();
        let mut background = Background::new();
        background.set(&cc.egui_ctx, config.background.as_deref());
        let mut app = App {
            config,
            audio,
            tracks: Vec::new(),
            scanning: None,
            cursor: 0,
            current: None,
            history: Vec::new(),
            dancer,
            background,
            mode: Mode::Library,
            pad: Pad::new(),
            host,
            toast: None,
        };
        app.rescan(&cc.egui_ctx);
        if !warnings.is_empty() {
            app.notify(warnings.join("\n"));
        }
        app
    }

    fn notify(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        eprintln!("{msg}");
        self.toast = Some((msg, Instant::now()));
    }

    fn rescan(&mut self, ctx: &Context) {
        let (tx, rx) = mpsc::channel();
        let folder = self.config.music_folder();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(music::scan(&folder));
            ctx.request_repaint();
        });
        self.scanning = Some(rx);
    }

    fn poll_scan(&mut self) {
        let Some(rx) = &self.scanning else { return };
        let Ok(tracks) = rx.try_recv() else { return };
        self.scanning = None;
        let playing = self.current.and_then(|i| self.tracks.get(i)).map(|t| t.path.clone());
        self.tracks = tracks;
        self.current = playing.and_then(|p| self.tracks.iter().position(|t| t.path == p));
        self.history.clear();
        self.cursor = self.current.unwrap_or(0);
    }

    fn play(&mut self, index: usize) {
        let Some(track) = self.tracks.get(index) else { return };
        let path = track.path.clone();
        let Ok(audio) = &mut self.audio else {
            let e = self.audio.as_ref().err().cloned().unwrap_or_default();
            return self.notify(format!("Can't play: {e}"));
        };
        match audio.play(&path) {
            Ok(()) => {
                if let Some(prev) = self.current.filter(|&p| p != index) {
                    self.history.push(prev);
                }
                if self.current != Some(index) {
                    self.dancer.pick(&self.tracks[index]);
                }
                self.current = Some(index);
            }
            Err(e) => self.notify(format!("{e:#}")),
        }
    }

    fn next(&mut self, auto: bool) {
        let count = self.tracks.len();
        if count == 0 {
            return;
        }
        let Some(current) = self.current else { return self.play(self.cursor.min(count - 1)) };
        if auto && self.config.repeat == Repeat::One {
            return self.play(current);
        }
        let next = if self.config.shuffle && count > 1 {
            let mut i = rand::random_range(0..count - 1);
            if i >= current {
                i += 1;
            }
            i
        } else if current + 1 < count {
            current + 1
        } else if !auto || self.config.repeat == Repeat::All {
            0
        } else {
            if let Ok(audio) = &mut self.audio {
                audio.stop();
            }
            self.current = None;
            return;
        };
        self.play(next);
        self.cursor = next;
    }

    fn previous(&mut self) {
        let Some(current) = self.current else { return };
        let played = self.audio.as_ref().map(|a| a.position()).unwrap_or_default();
        let prev = if played > RESTART_AFTER {
            current
        } else if self.config.shuffle {
            match self.history.pop() {
                Some(p) => {
                    self.current = None;
                    p
                }
                None => current,
            }
        } else {
            current.checked_sub(1).unwrap_or(self.tracks.len() - 1)
        };
        self.play(prev);
        self.cursor = prev;
    }

    fn set_volume(&mut self, delta: f32) {
        self.config.volume = ((self.config.volume + delta) * 20.0).round().clamp(0.0, 20.0) / 20.0;
        if let Ok(audio) = &self.audio {
            audio.set_volume(self.config.volume);
        }
        self.config.save();
    }

    fn open_browser(&mut self, from_settings: bool) {
        let dir = self.config.music_folder();
        let entries = music::subfolders(&dir);
        self.mode = Mode::Browse { dir, entries, cursor: 0, pick: Pick::Music { from_settings } };
    }

    fn open_background_picker(&mut self) {
        let dir = background::start_dir(self.config.background.as_deref());
        let entries = background::entries(&dir);
        let cursor = self.config.background.as_ref().and_then(|b| entries.iter().position(|e| e == b)).map_or(0, |i| i + 1 + dir.parent().is_some() as usize);
        self.mode = Mode::Browse { dir, entries, cursor, pick: Pick::Background };
    }

    fn set_background(&mut self, ctx: &Context, path: Option<PathBuf>) {
        self.background.set(ctx, path.as_deref());
        self.config.background = path;
        self.config.save();
        self.mode = Mode::Settings { cursor: BACKGROUND_ROW };
    }

    fn layout(&self) -> Layout {
        self.config.prompts.layout(self.pad.layout())
    }

    fn handle_settings(&mut self, cmd: Command, cursor: usize) {
        match cmd {
            Command::Up => self.mode = Mode::Settings { cursor: cursor.saturating_sub(1) },
            Command::Down => self.mode = Mode::Settings { cursor: (cursor + 1).min(SETTINGS_ROWS - 1) },
            Command::PageUp => self.mode = Mode::Settings { cursor: 0 },
            Command::PageDown => self.mode = Mode::Settings { cursor: SETTINGS_ROWS - 1 },
            Command::Confirm | Command::SeekBack | Command::SeekForward => match cursor {
                0 if cmd != Command::SeekBack => self.open_browser(true),
                0 => {}
                BACKGROUND_ROW if cmd != Command::SeekBack => self.open_background_picker(),
                BACKGROUND_ROW => {}
                2 => {
                    self.config.prompts = self.config.prompts.next(cmd != Command::SeekBack);
                    self.config.save();
                }
                _ => {
                    self.config.button_images ^= true;
                    self.config.save();
                }
            },
            Command::ChooseFolder => self.open_browser(true),
            Command::Back | Command::Settings => self.mode = Mode::Library,
            _ => {}
        }
    }

    fn handle(&mut self, ctx: &Context, cmd: Command) {
        match cmd {
            Command::PlayPause => {
                match (&self.audio, self.current) {
                    (Ok(audio), Some(_)) => audio.toggle_pause(),
                    _ => self.play(self.cursor),
                }
                return;
            }
            Command::Previous => return self.previous(),
            Command::Next => return self.next(false),
            Command::VolumeDown => return self.set_volume(-VOLUME_STEP),
            Command::VolumeUp => return self.set_volume(VOLUME_STEP),
            Command::Shuffle => {
                self.config.shuffle ^= true;
                self.config.save();
                return self.notify(if self.config.shuffle { "Shuffle on" } else { "Shuffle off" });
            }
            Command::Repeat => {
                self.config.repeat = self.config.repeat.next();
                self.config.save();
                return self.notify(self.config.repeat.label());
            }
            Command::Minimize => return self.host.minimize(ctx),
            _ => {}
        }
        if let Mode::Settings { cursor } = self.mode {
            return self.handle_settings(cmd, cursor);
        }
        match &mut self.mode {
            Mode::Library => {
                let last = self.tracks.len().saturating_sub(1);
                match cmd {
                    Command::Up => self.cursor = self.cursor.saturating_sub(1),
                    Command::Down => self.cursor = (self.cursor + 1).min(last),
                    Command::PageUp => self.cursor = self.cursor.saturating_sub(10),
                    Command::PageDown => self.cursor = (self.cursor + 10).min(last),
                    Command::SeekBack | Command::SeekForward => {
                        if let (Ok(audio), Some(_)) = (&self.audio, self.current) {
                            audio.seek_by(if cmd == Command::SeekBack { -SEEK_STEP } else { SEEK_STEP });
                        }
                    }
                    Command::Confirm => self.play(self.cursor),
                    Command::ChooseFolder => self.open_browser(false),
                    Command::Settings => self.mode = Mode::Settings { cursor: 0 },
                    Command::Back => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    _ => {}
                }
            }
            Mode::Browse { dir, entries, cursor, pick } => {
                let has_parent = dir.parent().is_some();
                let rows = 1 + has_parent as usize + entries.len();
                let pick = *pick;
                let list = |dir: &std::path::Path| if pick == Pick::Background { background::entries(dir) } else { music::subfolders(dir) };
                match cmd {
                    Command::Up => *cursor = cursor.saturating_sub(1),
                    Command::Down => *cursor = (*cursor + 1).min(rows - 1),
                    Command::PageUp => *cursor = cursor.saturating_sub(10),
                    Command::PageDown => *cursor = (*cursor + 10).min(rows - 1),
                    Command::Confirm if *cursor == 0 && pick == Pick::Background => self.set_background(ctx, None),
                    Command::Confirm if *cursor == 0 => {
                        self.config.music_folder = Some(dir.clone());
                        self.config.save();
                        self.mode = Mode::Library;
                        self.rescan(ctx);
                    }
                    Command::Confirm | Command::SeekForward => {
                        let target = if has_parent && *cursor == 1 {
                            dir.parent().map(|p| p.to_path_buf())
                        } else {
                            entries.get(*cursor - 1 - has_parent as usize).cloned()
                        };
                        if let Some(target) = target.as_ref().filter(|t| background::is_image(t)) {
                            if cmd == Command::Confirm {
                                let target = target.clone();
                                self.set_background(ctx, Some(target));
                            }
                        } else if let Some(target) = target {
                            *entries = list(&target);
                            *dir = target;
                            *cursor = 0;
                        }
                    }
                    Command::SeekBack => {
                        if let Some(parent) = dir.parent().map(|p| p.to_path_buf()) {
                            let from = dir.clone();
                            *entries = list(&parent);
                            *cursor = entries.iter().position(|f| *f == from).map_or(0, |i| i + 2);
                            *dir = parent;
                        }
                    }
                    Command::Back | Command::ChooseFolder | Command::Settings => {
                        self.mode = match pick {
                            Pick::Music { from_settings: true } => Mode::Settings { cursor: 0 },
                            Pick::Music { from_settings: false } => Mode::Library,
                            Pick::Background => Mode::Settings { cursor: BACKGROUND_ROW },
                        };
                    }
                    _ => {}
                }
            }
            Mode::Settings { .. } => {}
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.host.update(ctx);
        self.poll_scan();
        if let Some(e) = self.background.poll(ctx) {
            self.notify(e);
        }
        if self.current.is_some()
            && let Ok(audio) = &self.audio
            && !audio.is_paused()
            && audio.finished()
        {
            self.next(true);
        }
        if ctx.input(|i| i.viewport().minimized) == Some(true) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        for cmd in self.pad.poll(&ctx) {
            self.handle(&ctx, cmd);
        }

        let screen = ui.max_rect();
        let s = (screen.height() / 1080.0).max(0.4);
        let painter = ui.painter().clone();
        self.background.paint(&painter, screen);
        self.draw_top_bar(&painter, screen, s);
        let body = Rect::from_min_max(pos2(screen.left() + 60.0 * s, screen.top() + 110.0 * s), pos2(screen.right() - 60.0 * s, screen.bottom() - 90.0 * s));
        let list = Rect::from_min_max(body.min, pos2(body.left() + body.width() * 0.56, body.bottom()));
        let now = Rect::from_min_max(pos2(list.right() + 40.0 * s, body.top()), body.max);
        match self.mode {
            Mode::Library => self.draw_tracks(ui, list, s),
            Mode::Browse { .. } => self.draw_browser(ui, list, s),
            Mode::Settings { .. } => self.draw_settings(ui, list, s),
        }
        self.draw_now_playing(ui, now, s);
        if let Some(cmd) = self.draw_hints(ui, screen, s) {
            self.handle(&ctx, cmd);
        }
        if self.draw_minimize(ui, screen, s) {
            self.host.minimize(&ctx);
        }
        self.draw_toast(&painter, screen, s);

        let playing = self.current.is_some() && self.audio.as_ref().is_ok_and(|a| !a.is_paused());
        let focused = ctx.input(|i| i.focused);
        let every = if self.pad.busy() || (focused && playing) { 16 } else if focused { 50 } else { 250 };
        ctx.request_repaint_after(Duration::from_millis(every));
    }

    fn on_exit(&mut self) {
        self.config.save();
    }
}

impl App {
    fn draw_top_bar(&self, painter: &Painter, screen: Rect, s: f32) {
        let y = screen.top() + 55.0 * s;
        outlined_text(painter, pos2(screen.left() + 60.0 * s, y), "WitchyFiPlaya", FontId::new(30.0 * s, title_family()), TITLE, s);
        let folder = self.config.music_folder().display().to_string();
        let count = match self.scanning {
            Some(_) => "Scanning…".to_owned(),
            None => format!("{} track{}", self.tracks.len(), if self.tracks.len() == 1 { "" } else { "s" }),
        };
        let info = format!("{count}  ·  {folder}");
        painter.text(pos2(screen.left() + 330.0 * s, y + 2.0 * s), Align2::LEFT_CENTER, info, FontId::proportional(18.0 * s), Color32::from_gray(150));
        let clock = clock_text();
        painter.text(pos2(screen.right() - 60.0 * s, y), Align2::RIGHT_CENTER, clock, FontId::proportional(28.0 * s), Color32::WHITE);
    }

    fn draw_minimize(&self, ui: &Ui, screen: Rect, s: f32) -> bool {
        let (layout, images) = (self.layout(), self.config.button_images);
        let y = screen.bottom() - 45.0 * s;
        let size = prompts::size(ui, Button::Mode, layout, images, s);
        let button = Rect::from_min_size(pos2(screen.left() + 60.0 * s, y - size.y / 2.0), size);
        let label = if self.host.inside_yasaff { "YASAFF" } else { "Minimize" };
        let font = FontId::proportional(17.0 * s);
        let label_w = ui.painter().layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE).size().x;
        let rect = Rect::from_min_max(button.min, pos2(button.right() + 8.0 * s + label_w, button.bottom()));
        let guide = prompts::name(Button::Mode, layout);
        let tip = if self.host.inside_yasaff { format!("Back to YASAFF ({guide}, M): the music keeps playing") } else { format!("Minimize ({guide}, M)") };
        let response = ui.interact(rect, Id::new("minimize"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip);
        let hot = response.hovered();
        prompts::paint(ui, button, Button::Mode, layout, images, hot, s);
        let color = if hot { Color32::WHITE } else { Color32::from_gray(210) };
        ui.painter().text(pos2(button.right() + 8.0 * s, y), Align2::LEFT_CENTER, label, font, color);
        response.clicked()
    }

    fn draw_tracks(&mut self, ui: &Ui, rect: Rect, s: f32) {
        let painter = ui.painter();
        painter.rect_filled(rect, (8.0 * s) as u8, PANEL.gamma_multiply(0.9));
        if self.tracks.is_empty() {
            let (title, sub) = match self.scanning {
                Some(_) => ("Looking for music…", String::new()),
                None => {
                    let settings = prompts::name(Button::Start, self.layout());
                    ("No music files here", format!("{}\nChoose another folder in Settings: {settings} (O).", self.config.music_folder().display()))
                }
            };
            painter.text(rect.center() - vec2(0.0, 20.0 * s), Align2::CENTER_CENTER, title, FontId::proportional(26.0 * s), Color32::WHITE);
            let galley = painter.layout(sub, FontId::proportional(17.0 * s), Color32::from_gray(150), rect.width() - 80.0 * s);
            painter.galley(rect.center() + vec2(-galley.size().x / 2.0, 14.0 * s), galley, Color32::from_gray(150));
            return;
        }
        let row_h = 70.0 * s;
        let inner = rect.shrink(12.0 * s);
        let rows = ((inner.height() / row_h).floor() as usize).max(1);
        self.cursor = self.cursor.min(self.tracks.len() - 1);
        let first = self.cursor.saturating_sub(rows / 2).min(self.tracks.len().saturating_sub(rows));
        let scroll = ui.ctx().animate_value_with_time(Id::new("track-scroll"), first as f32, 0.12);
        let clip = painter.with_clip_rect(inner);
        let mut clicked = None;
        let start = scroll.floor() as usize;
        for i in start..(start + rows + 1).min(self.tracks.len()) {
            let track = &self.tracks[i];
            let top = inner.top() + (i as f32 - scroll) * row_h;
            let r = Rect::from_min_size(pos2(inner.left(), top), vec2(inner.width(), row_h - 6.0 * s));
            let playing = self.current == Some(i);
            if i == self.cursor {
                clip.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(24));
                clip.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
            }
            let number = if playing { self.play_glyph().to_owned() } else { format!("{}", i + 1) };
            let color = if playing { ACCENT } else { Color32::from_gray(120) };
            clip.text(pos2(r.left() + 52.0 * s, r.center().y), Align2::RIGHT_CENTER, number, FontId::proportional(18.0 * s), color);
            let text_x = r.left() + 76.0 * s;
            let title_color = if playing { ACCENT } else { Color32::WHITE };
            let row_clip = clip.with_clip_rect(r.intersect(inner));
            row_clip.text(pos2(text_x, r.top() + 10.0 * s), Align2::LEFT_TOP, &track.title, FontId::proportional(22.0 * s), title_color);
            row_clip.text(pos2(text_x, r.top() + 39.0 * s), Align2::LEFT_TOP, track.subtitle(), FontId::proportional(15.0 * s), Color32::from_gray(140));
            let visible = r.intersect(inner);
            if visible.is_positive() && ui.interact(visible, Id::new(("track", i)), Sense::click()).clicked() {
                clicked = Some(i);
            }
        }
        if let Some(i) = clicked {
            if i == self.cursor {
                self.play(i);
            } else {
                self.cursor = i;
            }
        }
    }

    fn draw_browser(&mut self, ui: &Ui, rect: Rect, s: f32) {
        let Mode::Browse { dir, entries, cursor, pick } = &self.mode else { return };
        let painter = ui.painter();
        painter.rect_filled(rect, (8.0 * s) as u8, PANEL);
        let (title, first) = match pick {
            Pick::Background => ("Choose the background", "🎨  Purple gradient (no picture)"),
            Pick::Music { .. } => ("Choose the music folder", "✔  Use this folder"),
        };
        painter.text(rect.left_top() + vec2(30.0 * s, 26.0 * s), Align2::LEFT_TOP, title, FontId::proportional(26.0 * s), Color32::WHITE);
        let clip = painter.with_clip_rect(rect.shrink(8.0 * s));
        clip.text(rect.left_top() + vec2(30.0 * s, 66.0 * s), Align2::LEFT_TOP, dir.display().to_string(), FontId::proportional(16.0 * s), Color32::from_gray(150));

        let mut labels = vec![(first.to_owned(), PAD_A)];
        if dir.parent().is_some() {
            labels.push(("⬆  ..".to_owned(), Color32::from_gray(200)));
        }
        labels.extend(entries.iter().map(|f| {
            let icon = if f.is_dir() { "🗀" } else { "🖼" };
            let color = if self.config.background.as_ref() == Some(f) { ACCENT } else { Color32::WHITE };
            (format!("{icon}  {}", f.file_name().unwrap_or_default().to_string_lossy()), color)
        }));
        let list = Rect::from_min_max(rect.left_top() + vec2(12.0 * s, 110.0 * s), rect.max - vec2(12.0 * s, 12.0 * s));
        let row_h = 56.0 * s;
        let rows = ((list.height() / row_h).floor() as usize).max(1);
        let first = cursor.saturating_sub(rows / 2).min(labels.len().saturating_sub(rows));
        let clip = painter.with_clip_rect(list);
        let mut clicked = None;
        for (i, (label, color)) in labels.iter().enumerate().skip(first).take(rows) {
            let r = Rect::from_min_size(pos2(list.left(), list.top() + (i - first) as f32 * row_h), vec2(list.width(), row_h - 6.0 * s));
            if i == *cursor {
                clip.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(24));
                clip.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
            }
            clip.text(pos2(r.left() + 26.0 * s, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(21.0 * s), *color);
            if ui.interact(r, Id::new(("folder", i)), Sense::click()).clicked() {
                clicked = Some(i);
            }
        }
        if let Some(i) = clicked {
            let same = i == *cursor;
            if let Mode::Browse { cursor, .. } = &mut self.mode {
                *cursor = i;
            }
            if same {
                self.handle(ui.ctx(), Command::Confirm);
            }
        }
    }

    fn draw_settings(&mut self, ui: &Ui, rect: Rect, s: f32) {
        let Mode::Settings { cursor } = self.mode else { return };
        let painter = ui.painter();
        painter.rect_filled(rect, (8.0 * s) as u8, PANEL);
        painter.text(rect.left_top() + vec2(30.0 * s, 26.0 * s), Align2::LEFT_TOP, "Settings", FontId::proportional(26.0 * s), Color32::WHITE);

        let prompts = match self.config.prompts {
            Prompts::Auto => format!("Automatic (now {})", if self.pad.layout() == Layout::Xbox { "Xbox" } else { "PlayStation" }),
            Prompts::Xbox => "Xbox".to_owned(),
            Prompts::PlayStation => "PlayStation".to_owned(),
        };
        let icons = if self.config.button_images { "Pictures" } else { "Generated" };
        let rows: [(&str, String, bool); SETTINGS_ROWS] = [
            ("Music folder", self.config.music_folder().display().to_string(), false),
            ("Background", self.config.background.as_ref().map_or("Purple gradient".to_owned(), |b| b.display().to_string()), false),
            ("Button prompts", prompts, true),
            ("Button icons", icons.to_owned(), true),
        ];
        let list = Rect::from_min_max(rect.left_top() + vec2(12.0 * s, 80.0 * s), rect.max - vec2(12.0 * s, 12.0 * s));
        let row_h = 76.0 * s;
        let clip = painter.with_clip_rect(list);
        let mut clicked = None;
        for (i, (title, value, cycles)) in rows.iter().enumerate() {
            let r = Rect::from_min_size(pos2(list.left(), list.top() + i as f32 * row_h), vec2(list.width(), row_h - 6.0 * s));
            if i == cursor {
                clip.rect_filled(r, (4.0 * s) as u8, Color32::from_white_alpha(24));
                clip.rect_filled(Rect::from_min_size(r.min, vec2(5.0 * s, r.height())), 0, ACCENT);
            }
            let row_clip = clip.with_clip_rect(r.intersect(list).shrink2(vec2(0.0, 2.0 * s)));
            row_clip.text(pos2(r.left() + 26.0 * s, r.top() + 10.0 * s), Align2::LEFT_TOP, *title, FontId::proportional(22.0 * s), Color32::WHITE);
            let value = if *cycles { format!("◀  {value}  ▶") } else { value.clone() };
            let color = if i == cursor { Color32::from_gray(200) } else { Color32::from_gray(140) };
            row_clip.text(pos2(r.left() + 26.0 * s, r.top() + 41.0 * s), Align2::LEFT_TOP, value, FontId::proportional(16.0 * s), color);
            if r.intersect(list).is_positive() && ui.interact(r.intersect(list), Id::new(("setting", i)), Sense::click()).clicked() {
                clicked = Some(i);
            }
        }
        if let Some(i) = clicked {
            self.mode = Mode::Settings { cursor: i };
            if i == cursor {
                self.handle(ui.ctx(), Command::Confirm);
            }
        }
    }

    fn play_glyph(&self) -> &'static str {
        match &self.audio {
            Ok(a) if a.is_paused() => "⏸",
            _ => "▶",
        }
    }

    fn draw_now_playing(&mut self, ui: &Ui, rect: Rect, s: f32) {
        let pad = 28.0 * s;
        let track = self.current.and_then(|i| self.tracks.get(i));
        let subtitle = track.map(Track::subtitle).filter(|t| !t.is_empty());
        let content_h = match track {
            None => 76.0 + if self.audio.is_err() { 40.0 } else { 0.0 },
            Some(_) => 198.0 + if subtitle.is_some() { 40.0 } else { 0.0 },
        } * s;
        let card = Rect::from_min_max(pos2(rect.left(), rect.bottom() - content_h - 2.0 * pad), rect.max);
        let stage = Rect::from_min_max(rect.min, pos2(rect.right(), card.top() - 12.0 * s));
        let paused = self.audio.as_ref().map_or(true, |a| a.is_paused());
        if track.is_some() && !paused {
            let played = self.audio.as_ref().map_or(0.0, |a| a.position().as_secs_f32());
            self.dancer.paint(ui, stage, played);
        } else {
            self.dancer.paint_sleep(ui, stage, ui.input(|i| i.time) as f32);
        }

        let painter = ui.painter();
        painter.rect_filled(card, (8.0 * s) as u8, PANEL.gamma_multiply(0.9));
        let inner = card.shrink(pad);
        let mut y = inner.top();
        let clip = painter.with_clip_rect(card.shrink2(vec2(pad / 2.0, 0.0)));
        let Some(track) = track else {
            clip.text(pos2(inner.center().x, y), Align2::CENTER_TOP, "Nothing playing", FontId::proportional(26.0 * s), Color32::from_gray(170));
            let hint = format!("{} plays the selected track", prompts::name(Button::South, self.layout()));
            clip.text(pos2(inner.center().x, y + 44.0 * s), Align2::CENTER_TOP, hint, FontId::proportional(17.0 * s), Color32::from_gray(120));
            if let Err(e) = &self.audio {
                clip.text(pos2(inner.center().x, y + 84.0 * s), Align2::CENTER_TOP, format!("No sound: {e}"), FontId::proportional(16.0 * s), PAD_B);
            }
            return;
        };
        clip.text(pos2(inner.left(), y), Align2::LEFT_TOP, &track.title, FontId::proportional(32.0 * s), Color32::WHITE);
        if let Some(subtitle) = subtitle {
            y += 44.0 * s;
            clip.text(pos2(inner.left(), y), Align2::LEFT_TOP, subtitle, FontId::proportional(19.0 * s), Color32::from_gray(170));
            y += 46.0 * s;
        } else {
            y += 50.0 * s;
        }

        let (pos, len) = match &self.audio {
            Ok(a) => (a.position(), a.duration()),
            Err(_) => (Duration::ZERO, None),
        };
        let bar = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 8.0 * s));
        painter.rect_filled(bar, 4, Color32::from_white_alpha(30));
        if let Some(len) = len.filter(|l| !l.is_zero()) {
            let t = (pos.as_secs_f32() / len.as_secs_f32()).clamp(0.0, 1.0);
            painter.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * t, bar.height())), 4, ACCENT);
            painter.circle_filled(pos2(bar.left() + bar.width() * t, bar.center().y), 9.0 * s, Color32::WHITE);
        }
        y += 22.0 * s;
        painter.text(pos2(inner.left(), y), Align2::LEFT_TOP, time_text(pos), FontId::proportional(16.0 * s), Color32::from_gray(160));
        let total = len.map_or("–:––".to_owned(), time_text);
        painter.text(pos2(inner.right(), y), Align2::RIGHT_TOP, total, FontId::proportional(16.0 * s), Color32::from_gray(160));
        y += 44.0 * s;

        let font = FontId::proportional(20.0 * s);
        let mut x = inner.left();
        let chips = [
            (format!("{}  {}", self.play_glyph(), if self.play_glyph() == "⏸" { "Paused" } else { "Playing" }), true),
            ("🔀  Shuffle".to_owned(), self.config.shuffle),
            (format!("{}  {}", if self.config.repeat == Repeat::One { "🔂" } else { "🔁" }, self.config.repeat.label()), self.config.repeat != Repeat::Off),
        ];
        for (label, on) in chips {
            let galley = painter.layout_no_wrap(label, font.clone(), Color32::WHITE);
            let chip = Rect::from_min_size(pos2(x, y), galley.size() + vec2(24.0 * s, 14.0 * s));
            painter.rect_filled(chip, (chip.height() / 2.0) as u8, if on { ACCENT.gamma_multiply(0.8) } else { Color32::from_white_alpha(18) });
            let color = if on { Color32::WHITE } else { Color32::from_gray(140) };
            painter.galley(chip.center() - galley.size() / 2.0, galley, color);
            x = chip.right() + 10.0 * s;
        }
        y += 64.0 * s;
        painter.text(pos2(inner.left(), y + 8.0 * s), Align2::LEFT_CENTER, "🔊", FontId::proportional(22.0 * s), Color32::WHITE);
        let vol = Rect::from_min_size(pos2(inner.left() + 40.0 * s, y + 4.0 * s), vec2(inner.width() - 110.0 * s, 8.0 * s));
        painter.rect_filled(vol, 4, Color32::from_white_alpha(30));
        painter.rect_filled(Rect::from_min_size(vol.min, vec2(vol.width() * self.config.volume, vol.height())), 4, Color32::WHITE);
        painter.text(pos2(inner.right(), y + 8.0 * s), Align2::RIGHT_CENTER, format!("{:.0} %", self.config.volume * 100.0), FontId::proportional(17.0 * s), Color32::from_gray(170));
    }

    fn draw_hints(&self, ui: &Ui, screen: Rect, s: f32) -> Option<Command> {
        type Hint = (&'static [(Button, Command)], &'static [&'static str]);
        let hints: &[Hint] = match self.mode {
            Mode::Library => &[
                (&[(Button::South, Command::Confirm)], &["Play"]),
                (&[(Button::West, Command::PlayPause)], &["Pause"]),
                (&[(Button::LeftTrigger, Command::Previous), (Button::RightTrigger, Command::Next)], &["Prev", "Next"]),
                (&[(Button::DPadLeft, Command::SeekBack), (Button::DPadRight, Command::SeekForward)], &["Seek"]),
                (&[(Button::LeftTrigger2, Command::VolumeDown), (Button::RightTrigger2, Command::VolumeUp)], &["Volume"]),
                (&[(Button::North, Command::Shuffle)], &["Shuffle"]),
                (&[(Button::Select, Command::Repeat)], &["Repeat"]),
                (&[(Button::Start, Command::Settings)], &["Settings"]),
                (&[(Button::East, Command::Back)], &["Quit"]),
            ],
            Mode::Browse { .. } => &[
                (&[(Button::South, Command::Confirm)], &["Open"]),
                (&[(Button::DPadLeft, Command::SeekBack)], &["Up a folder"]),
                (&[(Button::East, Command::Back)], &["Cancel"]),
            ],
            Mode::Settings { .. } => &[
                (&[(Button::South, Command::Confirm)], &["Choose"]),
                (&[(Button::DPadLeft, Command::SeekBack), (Button::DPadRight, Command::SeekForward)], &["Change"]),
                (&[(Button::East, Command::Back)], &["Back"]),
            ],
        };
        let (layout, images) = (self.layout(), self.config.button_images);
        let painter = ui.painter();
        let font = FontId::proportional(17.0 * s);
        let layout_text = |text: &str, color| painter.layout_no_wrap(text.to_owned(), font.clone(), color);
        let slash_w = layout_text("/", Color32::WHITE).size().x;
        let label_color = Color32::from_gray(210);
        let mut x = screen.right() - 60.0 * s;
        let y = screen.bottom() - 45.0 * s;
        let mut clicked = None;
        for (h, (buttons, labels)) in hints.iter().enumerate().rev() {
            let mut label_rects = vec![Rect::NOTHING; labels.len()];
            for (i, label) in labels.iter().enumerate().rev() {
                if i + 1 < labels.len() {
                    x -= slash_w;
                    painter.text(pos2(x, y), Align2::LEFT_CENTER, "/", font.clone(), label_color);
                }
                let w = layout_text(label, label_color).size().x;
                x -= w;
                label_rects[i] = Rect::from_min_size(pos2(x, y - 14.0 * s), vec2(w, 28.0 * s));
            }
            x -= 8.0 * s;
            let mut button_rects = vec![Rect::NOTHING; buttons.len()];
            for (i, (button, _)) in buttons.iter().enumerate().rev() {
                if i + 1 < buttons.len() {
                    x -= 4.0 * s;
                }
                let size = prompts::size(ui, *button, layout, images, s);
                x -= size.x;
                button_rects[i] = Rect::from_min_size(pos2(x, y - size.y / 2.0), size);
            }
            x -= 26.0 * s;

            let own_labels = labels.len() == buttons.len();
            let mut hovered = vec![false; buttons.len()];
            for (i, (_, cmd)) in buttons.iter().enumerate() {
                let targets = if own_labels { vec![button_rects[i], label_rects[i]] } else { vec![button_rects[i]] };
                for (t, rect) in targets.into_iter().enumerate() {
                    let response = ui.interact(rect, Id::new(("hint", h, i, t)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                    hovered[i] |= response.hovered();
                    if response.clicked() {
                        clicked = Some(*cmd);
                    }
                }
            }
            for (i, (button, _)) in buttons.iter().enumerate() {
                prompts::paint(ui, button_rects[i], *button, layout, images, hovered[i], s);
            }
            for (i, label) in labels.iter().enumerate() {
                let hot = if own_labels { hovered[i] } else { hovered.contains(&true) };
                let color = if hot { Color32::WHITE } else { label_color };
                let galley = layout_text(label, color);
                painter.galley(pos2(label_rects[i].left(), y - galley.size().y / 2.0), galley, color);
            }
        }
        clicked
    }

    fn draw_toast(&mut self, painter: &Painter, screen: Rect, s: f32) {
        let Some((msg, at)) = &self.toast else { return };
        if at.elapsed() > TOAST_TIME {
            self.toast = None;
            return;
        }
        let galley = painter.layout(msg.clone(), FontId::proportional(20.0 * s), Color32::WHITE, screen.width() * 0.6);
        let rect = Rect::from_center_size(pos2(screen.center().x, screen.bottom() - 120.0 * s), galley.size() + vec2(40.0 * s, 24.0 * s));
        painter.rect_filled(rect, (6.0 * s) as u8, Color32::from_black_alpha(220));
        painter.rect_stroke(rect, (6.0 * s) as u8, Stroke::new(1.0, Color32::from_white_alpha(30)), egui::StrokeKind::Inside);
        painter.galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
    }
}

fn outlined_text(painter: &Painter, pos: Pos2, text: &str, font: FontId, color: Color32, s: f32) {
    let (border, shadow) = (1.5 * s, 2.0 * s);
    let draw = |offset: Vec2, color: Color32| {
        painter.text(pos + offset, Align2::LEFT_CENTER, text, font.clone(), color);
    };
    for i in 0..16 {
        let angle = i as f32 * std::f32::consts::TAU / 16.0;
        draw(vec2(shadow, shadow) + Vec2::angled(angle) * border, Color32::from_black_alpha(140));
    }
    for i in 0..16 {
        draw(Vec2::angled(i as f32 * std::f32::consts::TAU / 16.0) * border, Color32::BLACK);
    }
    draw(Vec2::ZERO, color);
}

fn time_text(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn clock_text() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}
