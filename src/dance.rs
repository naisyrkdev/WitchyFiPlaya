use crate::music::Track;
use egui::{Color32, ColorImage, Context, Rect, TextureHandle, TextureOptions, Ui, pos2, vec2};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

const DEFAULT_FPS: f32 = 8.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Logic {
    ByTrackThenBase,
    ByTrackOnly,
    TotalRandom,
    ByTrackThenRandomFromRandomPool,
    Repeatly,
}

impl Logic {
    fn parse(name: &str) -> Result<Self, String> {
        let all = [
            ("ByTrackThenBase", Logic::ByTrackThenBase),
            ("ByTrackOnly", Logic::ByTrackOnly),
            ("TotalRandom", Logic::TotalRandom),
            ("ByTrackThenRandomFromRandomPool", Logic::ByTrackThenRandomFromRandomPool),
            ("Repeatly", Logic::Repeatly),
        ];
        if name.trim().is_empty() {
            return Ok(Logic::TotalRandom);
        }
        all.iter().find(|(n, _)| n.eq_ignore_ascii_case(name.trim())).map(|&(_, l)| l).ok_or_else(|| format!("unknown dancingLogic \"{name}\", using TotalRandom"))
    }
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Settings {
    dancing_animations: BTreeMap<String, AnimationSpec>,
    dancing_logic: String,
    base_animation: Option<String>,
    animation_per_tracks_objects: BTreeMap<String, TrackRule>,
    sleep_animation: Option<AnimationSpec>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AnimationSpec {
    src: String,
    #[serde(default = "one")]
    columns: u32,
    #[serde(default = "one")]
    rows: u32,
    frames: Option<u32>,
    fps: Option<f32>,
    frames_names: Option<Vec<String>>,
    format: Option<String>,
}

fn one() -> u32 {
    1
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct TrackRule {
    #[serde(rename = "Title", alias = "title")]
    title: Option<String>,
    #[serde(rename = "Author", alias = "author")]
    author: Option<String>,
    #[serde(rename = "File", alias = "file")]
    file: Option<String>,
    #[serde(rename = "Dancing", alias = "dancing")]
    dancing: String,
}

impl TrackRule {
    fn matches(&self, track: &Track) -> bool {
        let same = |want: &Option<String>, have: Option<&str>| match want.as_deref().map(str::trim).filter(|w| !w.is_empty()) {
            None => None,
            Some(w) => Some(have.is_some_and(|h| h.trim().eq_ignore_ascii_case(w))),
        };
        let file = track.path.file_name().map(|f| f.to_string_lossy());
        let checks = [same(&self.title, Some(&track.title)), same(&self.author, track.artist.as_deref()), same(&self.file, file.as_deref())];
        checks.iter().any(Option::is_some) && checks.iter().flatten().all(|&ok| ok)
    }
}

struct Animation {
    src: String,
    files: Vec<PathBuf>,
    columns: u32,
    rows: u32,
    frames: u32,
    fps: f32,
}

impl Animation {
    fn new(dir: &Path, spec: AnimationSpec, warnings: &mut Vec<String>) -> Self {
        let path = dir.join(&spec.src);
        let catalog = spec.frames_names.is_some() || spec.format.is_some() || path.is_dir();
        let files = if catalog { frame_files(&path, &spec, warnings) } else { vec![path] };
        let missing: Vec<_> = files.iter().filter(|f| !f.is_file()).collect();
        if let Some(first) = missing.first() {
            let more = if missing.len() > 1 { format!(" (and {} more)", missing.len() - 1) } else { String::new() };
            warnings.push(format!("{}: {} not found{more}", spec.src, first.display()));
        }
        let (columns, rows) = (spec.columns.max(1), spec.rows.max(1));
        let cells = columns * rows * files.len().max(1) as u32;
        Animation {
            src: spec.src,
            files,
            columns,
            rows,
            frames: spec.frames.unwrap_or(cells).clamp(1, cells),
            fps: spec.fps.filter(|f| *f > 0.0).unwrap_or(DEFAULT_FPS),
        }
    }

    fn exists(&self) -> bool {
        self.files.iter().any(|f| f.is_file())
    }
}

fn frame_files(path: &Path, spec: &AnimationSpec, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let format = spec.format.as_deref().map(|f| f.trim().trim_start_matches('.')).filter(|f| !f.is_empty()).unwrap_or("png");
    let file = |name: &str| {
        let has_extension = Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case(format));
        path.join(if has_extension { name.to_owned() } else { format!("{name}.{format}") })
    };
    if let Some(names) = &spec.frames_names {
        return names.iter().map(|n| file(n)).collect();
    }
    if let Some(frames) = spec.frames {
        return (1..=frames.max(1)).map(|n| file(&n.to_string())).collect();
    }
    let mut files: Vec<PathBuf> = match std::fs::read_dir(path) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case(format)))
            .collect(),
        Err(e) => {
            warnings.push(format!("Can't read the folder {}: {e}", path.display()));
            Vec::new()
        }
    };
    let stem = |p: &PathBuf| p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    files.sort_by_cached_key(|p| (stem(p).parse::<u64>().unwrap_or(u64::MAX), stem(p)));
    files
}

pub struct Dancer {
    animations: Vec<Animation>,
    dances: usize,
    sleep: Option<usize>,
    logic: Logic,
    base: Option<usize>,
    rules: Vec<(TrackRule, usize)>,
    current: Option<usize>,
    turn: usize,
    textures: HashMap<usize, Vec<TextureHandle>>,
    loading: HashMap<usize, mpsc::Receiver<Vec<Option<ColorImage>>>>,
}

impl Dancer {
    fn empty() -> Self {
        Dancer {
            animations: Vec::new(),
            dances: 0,
            sleep: None,
            logic: Logic::TotalRandom,
            base: None,
            rules: Vec::new(),
            current: None,
            turn: 0,
            textures: HashMap::new(),
            loading: HashMap::new(),
        }
    }

    pub fn load() -> (Self, Vec<String>) {
        let mut dancer = Dancer::empty();
        let Some(dir) = content_dir() else {
            return (dancer, vec!["No content folder with settings.json found, so nobody dances".to_owned()]);
        };
        let text = match std::fs::read_to_string(dir.join("settings.json")) {
            Ok(text) => text,
            Err(e) => return (dancer, vec![format!("Can't read content/settings.json: {e}")]),
        };
        let settings: Settings = match serde_json::from_str(&strip_comments(&text)) {
            Ok(s) => s,
            Err(e) => return (dancer, vec![format!("content/settings.json is broken: {e}")]),
        };
        let mut warnings = Vec::new();
        let names: Vec<String> = settings.dancing_animations.keys().cloned().collect();
        for spec in settings.dancing_animations.into_values() {
            dancer.animations.push(Animation::new(&dir, spec, &mut warnings));
        }
        dancer.dances = dancer.animations.len();
        let find = |name: &str| names.iter().position(|n| n == name).or_else(|| dancer.animations.iter().position(|a| a.src == name));
        dancer.logic = Logic::parse(&settings.dancing_logic).unwrap_or_else(|e| {
            warnings.push(e);
            Logic::TotalRandom
        });
        dancer.base = match settings.base_animation.as_deref() {
            Some(name) => find(name).or_else(|| {
                warnings.push(format!("baseAnimation \"{name}\" is not in dancingAnimations"));
                None
            }),
            None => None,
        };
        for (key, rule) in settings.animation_per_tracks_objects {
            match find(&rule.dancing) {
                Some(i) => dancer.rules.push((rule, i)),
                None => warnings.push(format!("{key}: animation \"{}\" is not in dancingAnimations", rule.dancing)),
            }
        }
        let spec = settings.sleep_animation.unwrap_or_else(|| AnimationSpec { src: "sleep.png".to_owned(), columns: 1, rows: 1, ..Default::default() });
        let mut sleep_warnings = Vec::new();
        let sleep = Animation::new(&dir, spec, &mut sleep_warnings);
        if sleep.exists() {
            warnings.extend(sleep_warnings);
            dancer.sleep = Some(dancer.animations.len());
            dancer.animations.push(sleep);
        } else {
            warnings.push(format!("{} not found, so nobody sleeps", dir.join(&sleep.src).display()));
        }
        (dancer, warnings)
    }

    pub fn pick(&mut self, track: &Track) {
        let count = self.dances;
        if count == 0 {
            return;
        }
        let assigned = self.rules.iter().find(|(rule, _)| rule.matches(track)).map(|&(_, i)| i);
        self.current = match self.logic {
            Logic::ByTrackOnly => assigned,
            Logic::ByTrackThenBase => assigned.or(self.base).or(Some(0)),
            Logic::TotalRandom => self.random((0..count).collect()),
            Logic::ByTrackThenRandomFromRandomPool => assigned.or_else(|| {
                let pool: Vec<usize> = (0..count).filter(|i| !self.rules.iter().any(|(_, r)| r == i)).collect();
                self.random(if pool.is_empty() { (0..count).collect() } else { pool })
            }),
            Logic::Repeatly => {
                self.turn += 1;
                Some((self.turn - 1) % count)
            }
        };
    }

    fn random(&self, mut pool: Vec<usize>) -> Option<usize> {
        if pool.len() > 1 {
            pool.retain(|&i| Some(i) != self.current);
        }
        (!pool.is_empty()).then(|| pool[rand::random_range(0..pool.len())])
    }

    pub fn paint(&mut self, ui: &Ui, rect: Rect, seconds: f32) -> bool {
        self.paint_animation(ui, self.current, rect, seconds)
    }

    pub fn paint_sleep(&mut self, ui: &Ui, rect: Rect, seconds: f32) -> bool {
        self.paint_animation(ui, self.sleep, rect, seconds)
    }

    fn paint_animation(&mut self, ui: &Ui, index: Option<usize>, rect: Rect, seconds: f32) -> bool {
        let Some(index) = index else { return false };
        if !rect.is_positive() {
            return false;
        }
        let Some(textures) = self.textures(ui.ctx(), index) else { return false };
        let anim = &self.animations[index];
        let cells = anim.columns * anim.rows;
        let frames = anim.frames.min(cells * textures.len() as u32);
        let frame = (seconds.max(0.0) * anim.fps) as u32 % frames;
        let (texture, cell) = (&textures[(frame / cells) as usize], frame % cells);
        let (col, row) = ((cell % anim.columns) as f32, (cell / anim.columns) as f32);
        let (cols, rows) = (anim.columns as f32, anim.rows as f32);
        let uv = Rect::from_min_max(pos2(col / cols, row / rows), pos2((col + 1.0) / cols, (row + 1.0) / rows));
        let cell = vec2(texture.size()[0] as f32 / cols, texture.size()[1] as f32 / rows);
        let scale = (rect.width() / cell.x).min(rect.height() / cell.y);
        let size = cell * scale;
        let target = Rect::from_min_size(pos2(rect.center().x - size.x / 2.0, rect.bottom() - size.y), size);
        ui.painter().image(texture.id(), target, uv, Color32::WHITE);
        true
    }

    fn textures(&mut self, ctx: &Context, index: usize) -> Option<Vec<TextureHandle>> {
        if let Some(textures) = self.textures.get(&index) {
            return (!textures.is_empty()).then(|| textures.clone());
        }
        let files = &self.animations[index].files;
        if let Some(rx) = self.loading.get(&index) {
            let images = rx.try_recv().ok()?;
            self.loading.remove(&index);
            let textures: Vec<TextureHandle> = files
                .iter()
                .zip(images)
                .filter_map(|(path, img)| Some(ctx.load_texture(path.display().to_string(), img?, TextureOptions::LINEAR)))
                .collect();
            self.textures.insert(index, textures);
            return self.textures(ctx, index);
        }
        let (tx, rx) = mpsc::channel();
        let files = files.clone();
        let max_side = ctx.input(|i| i.max_texture_side) as u32;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(files.iter().map(|path| decode(path, max_side)).collect());
            ctx.request_repaint();
        });
        self.loading.insert(index, rx);
        None
    }
}

pub fn decode(path: &Path, max_side: u32) -> Option<ColorImage> {
    let decoded = image::ImageReader::open(path).and_then(|r| r.with_guessed_format()).map_err(image::ImageError::from).and_then(|r| r.decode());
    let mut image = decoded.map_err(|e| eprintln!("can't load {}: {e}", path.display())).ok()?.to_rgba8();
    let (w, h) = image.dimensions();
    if w.max(h) > max_side {
        let scale = max_side as f32 / w.max(h) as f32;
        let size = (((w as f32 * scale) as u32).max(1), ((h as f32 * scale) as u32).max(1));
        image = image::imageops::resize(&image, size.0, size.1, image::imageops::FilterType::Triangle);
    }
    let size = [image.width() as usize, image.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, image.as_raw()))
}

fn content_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    let cwd = std::env::current_dir().ok();
    exe.iter()
        .flat_map(|e| e.ancestors().skip(1).take(4))
        .chain(cwd.as_deref())
        .map(|d| d.join("content"))
        .find(|d| d.join("settings.json").is_file())
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let (mut in_string, mut escaped) = (false, false);
        let mut end = line.len();
        let bytes = line.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                _ if escaped => escaped = false,
                b'\\' if in_string => escaped = true,
                b'"' => in_string ^= true,
                b'/' if !in_string && bytes.get(i + 1) == Some(&b'/') => {
                    end = i;
                    break;
                }
                _ => {}
            }
        }
        out.push_str(&line[..end]);
        out.push('\n');
    }
    out
}
