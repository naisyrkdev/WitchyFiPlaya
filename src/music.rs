use lofty::config::ParseOptions;
use lofty::file::{TaggedFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Accessor;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 12;
const EXTENSIONS: [&str; 11] = ["mp3", "flac", "wav", "ogg", "oga", "m4a", "aac", "aif", "aiff", "caf", "mka"];

#[derive(Clone, Debug)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
}

impl Track {
    fn read(path: PathBuf) -> Self {
        let tagged = read_tags(&path);
        let tag = tagged.as_ref().and_then(|t| t.primary_tag().or(t.first_tag()));
        let text = |s: Option<Cow<str>>| s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        let title = text(tag.and_then(|t| t.title())).unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned());
        Track { title, artist: text(tag.and_then(|t| t.artist())), album: text(tag.and_then(|t| t.album())), path }
    }

    pub fn subtitle(&self) -> String {
        [self.artist.as_deref(), self.album.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" · ")
    }
}

fn read_tags(path: &Path) -> Option<TaggedFile> {
    Probe::open(path).ok()?.options(ParseOptions::new().read_properties(false)).read().ok()
}

pub fn scan(root: &Path) -> Vec<Track> {
    let mut files = Vec::new();
    walk(root, 0, &mut files);
    files.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    files.into_iter().map(Track::read).collect()
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if hidden {
            continue;
        }
        if path.is_dir() {
            walk(&path, depth + 1, out);
        } else if path.extension().is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x))) {
            out.push(path);
        }
    }
}

pub fn subfolders(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort_by_key(|p| p.file_name().unwrap_or_default().to_string_lossy().to_lowercase());
    dirs
}
