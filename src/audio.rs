use anyhow::{Context, Result};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use std::fs::File;
use std::path::Path;
use std::time::Duration;

pub struct Audio {
    _device: MixerDeviceSink,
    player: Player,
    duration: Option<Duration>,
}

impl Audio {
    pub fn open(volume: f32) -> Result<Self> {
        let mut device = DeviceSinkBuilder::open_default_sink().context("no sound output device")?;
        device.log_on_drop(false);
        let player = Player::connect_new(device.mixer());
        player.set_volume(volume);
        Ok(Audio { _device: device, player, duration: None })
    }

    pub fn play(&mut self, path: &Path) -> Result<()> {
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let source = Decoder::try_from(file).with_context(|| format!("{} is not a playable audio file", path.display()))?;
        self.duration = source.total_duration();
        self.player.clear();
        self.player.append(source);
        self.player.play();
        Ok(())
    }

    pub fn stop(&mut self) {
        self.player.clear();
        self.duration = None;
    }

    pub fn toggle_pause(&self) {
        if self.player.is_paused() {
            self.player.play();
        } else {
            self.player.pause();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.player.is_paused()
    }

    pub fn finished(&self) -> bool {
        self.player.empty()
    }

    pub fn position(&self) -> Duration {
        self.player.get_pos()
    }

    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }

    pub fn seek_by(&self, delta: f32) {
        let pos = self.position().as_secs_f32() + delta;
        let end = self.duration.map_or(f32::MAX, |d| (d.as_secs_f32() - 0.5).max(0.0));
        let _ = self.player.try_seek(Duration::from_secs_f32(pos.clamp(0.0, end)));
    }

    pub fn set_volume(&self, volume: f32) {
        self.player.set_volume(volume);
    }
}
