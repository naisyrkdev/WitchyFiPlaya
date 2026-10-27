use egui::{Color32, ColorImage, Context, Painter, Rect, TextureHandle, TextureOptions, pos2};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

const TOP: Color32 = Color32::from_rgb(0x5b, 0x24, 0x8c);
const BOTTOM: Color32 = Color32::from_rgb(0x12, 0x06, 0x22);
const SHADE: Color32 = Color32::from_black_alpha(70);
const EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// The purple gradient, or a picture of the user's own covering the window.
pub struct Background {
    texture: Option<TextureHandle>,
    loading: Option<(PathBuf, mpsc::Receiver<Option<ColorImage>>)>,
}

impl Background {
    pub fn new() -> Self {
        Background { texture: None, loading: None }
    }

    pub fn set(&mut self, ctx: &Context, path: Option<&Path>) {
        self.texture = None;
        self.loading = None;
        let Some(path) = path else { return };
        let (tx, rx) = mpsc::channel();
        let (file, max_side, ctx) = (path.to_path_buf(), ctx.input(|i| i.max_texture_side) as u32, ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(crate::dance::decode(&file, max_side));
            ctx.request_repaint();
        });
        self.loading = Some((path.to_path_buf(), rx));
    }

    /// Picks up a finished load; returns an error message when the picture couldn't be read.
    pub fn poll(&mut self, ctx: &Context) -> Option<String> {
        let (path, rx) = self.loading.as_ref()?;
        let image = rx.try_recv().ok()?;
        let path = path.clone();
        self.loading = None;
        match image {
            Some(image) => {
                self.texture = Some(ctx.load_texture(path.display().to_string(), image, TextureOptions::LINEAR));
                None
            }
            None => Some(format!("Can't load the background {}, using the gradient", path.display())),
        }
    }

    pub fn paint(&self, painter: &Painter, rect: Rect) {
        let Some(texture) = &self.texture else { return gradient(painter, rect) };
        let size = texture.size_vec2();
        let scale = (rect.width() / size.x).max(rect.height() / size.y);
        let visible = rect.size() / (size * scale);
        let min = pos2((1.0 - visible.x) / 2.0, (1.0 - visible.y) / 2.0);
        painter.image(texture.id(), rect, Rect::from_min_max(min, pos2(1.0 - min.x, 1.0 - min.y)), Color32::WHITE);
        painter.rect_filled(rect, 0, SHADE);
    }
}

fn gradient(painter: &Painter, rect: Rect) {
    let mut mesh = egui::Mesh::default();
    for (pos, color) in [(rect.left_top(), TOP), (rect.right_top(), TOP), (rect.left_bottom(), BOTTOM), (rect.right_bottom(), BOTTOM)] {
        mesh.colored_vertex(pos, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(egui::Shape::mesh(mesh));
}

pub fn is_image(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Subfolders first, then the pictures in `dir`.
pub fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut images: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .map(|e| e.path())
                .filter(|p| is_image(p))
                .collect()
        })
        .unwrap_or_default();
    images.sort_by_key(|p| p.file_name().unwrap_or_default().to_string_lossy().to_lowercase());
    let mut all = crate::music::subfolders(dir);
    all.extend(images);
    all
}

/// Where the picker opens: next to the current picture, else the Pictures folder, else home.
pub fn start_dir(current: Option<&Path>) -> PathBuf {
    current
        .and_then(Path::parent)
        .filter(|d| d.is_dir())
        .map(Path::to_path_buf)
        .or_else(|| directories::UserDirs::new().and_then(|u| u.picture_dir().map(Path::to_path_buf)))
        .or_else(|| directories::UserDirs::new().map(|u| u.home_dir().to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}
