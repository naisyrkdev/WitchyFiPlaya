use crate::prompts::Layout;
use egui::{Context, Key};
use gilrs::{Axis, Button, EventType, Gamepad, Gilrs};
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Command {
    Up,
    Down,
    PageUp,
    PageDown,
    SeekBack,
    SeekForward,
    Confirm,
    Back,
    PlayPause,
    Previous,
    Next,
    VolumeDown,
    VolumeUp,
    Shuffle,
    Repeat,
    ChooseFolder,
    Settings,
    Minimize,
}

const STICK_THRESHOLD: f32 = 0.5;
const REPEAT_DELAY: Duration = Duration::from_millis(380);
const REPEAT_RATE: Duration = Duration::from_millis(90);

pub struct Pad {
    gilrs: Option<Gilrs>,
    held: Option<(Command, Instant)>,
    stick: Option<Command>,
    focused_since: Option<SystemTime>,
    layout: Layout,
}

impl Pad {
    pub fn new() -> Self {
        let gilrs = Gilrs::new().map_err(|e| eprintln!("no controller support: {e}")).ok();
        let layout = gilrs.as_ref().and_then(|g| g.gamepads().next()).map_or(Layout::Xbox, |(_, pad)| layout_of(pad));
        Pad { gilrs, held: None, stick: None, focused_since: None, layout }
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn poll(&mut self, ctx: &Context) -> Vec<Command> {
        let focused = ctx.input(|i| i.focused);
        if !focused {
            self.focused_since = None;
        } else if self.focused_since.is_none() {
            self.focused_since = Some(SystemTime::now());
        }
        let mut out = keyboard(ctx);
        let Some(gilrs) = &mut self.gilrs else { return out };
        while let Some(event) = gilrs.next_event() {
            if self.focused_since.is_none_or(|since| event.time < since) {
                continue;
            }
            match event.event {
                EventType::Connected => self.layout = layout_of(gilrs.gamepad(event.id)),
                EventType::ButtonPressed(button, _) => {
                    self.layout = layout_of(gilrs.gamepad(event.id));
                    if let Some(cmd) = button_command(button) {
                        out.push(cmd);
                        if repeats(cmd) {
                            self.held = Some((cmd, Instant::now() + REPEAT_DELAY));
                        }
                    }
                }
                EventType::ButtonReleased(button, _) if self.held.is_some_and(|(cmd, _)| Some(cmd) == button_command(button)) => {
                    self.held = None;
                }
                _ => {}
            }
        }
        if !focused {
            self.held = None;
            return out;
        }

        let stick = gilrs.gamepads().find_map(|(_, pad)| {
            let (x, y) = (pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
            if y > STICK_THRESHOLD {
                Some(Command::Up)
            } else if y < -STICK_THRESHOLD {
                Some(Command::Down)
            } else if x < -STICK_THRESHOLD {
                Some(Command::SeekBack)
            } else if x > STICK_THRESHOLD {
                Some(Command::SeekForward)
            } else {
                None
            }
        });
        if stick != self.stick {
            self.stick = stick;
            if let Some(cmd) = stick {
                out.push(cmd);
                self.held = Some((cmd, Instant::now() + REPEAT_DELAY));
            } else if self.held.is_some_and(|(cmd, _)| matches!(cmd, Command::Up | Command::Down | Command::SeekBack | Command::SeekForward)) {
                self.held = None;
            }
        }
        if let Some((cmd, at)) = &mut self.held
            && Instant::now() >= *at
        {
            out.push(*cmd);
            *at = Instant::now() + REPEAT_RATE;
        }
        out
    }

    pub fn busy(&self) -> bool {
        self.held.is_some()
    }
}

fn button_command(button: Button) -> Option<Command> {
    Some(match button {
        Button::DPadUp => Command::Up,
        Button::DPadDown => Command::Down,
        Button::DPadLeft => Command::SeekBack,
        Button::DPadRight => Command::SeekForward,
        Button::South => Command::Confirm,
        Button::East => Command::Back,
        Button::West => Command::PlayPause,
        Button::North => Command::Shuffle,
        Button::LeftTrigger => Command::Previous,
        Button::RightTrigger => Command::Next,
        Button::LeftTrigger2 => Command::VolumeDown,
        Button::RightTrigger2 => Command::VolumeUp,
        Button::Select => Command::Repeat,
        Button::Start => Command::Settings,
        Button::Mode => Command::Minimize,
        _ => return None,
    })
}

fn layout_of(pad: Gamepad) -> Layout {
    const SONY: u16 = 0x054c;
    let name = pad.name().to_lowercase();
    let sony = ["playstation", "dualshock", "dualsense", "sony", "ps3", "ps4", "ps5"].iter().any(|n| name.contains(n));
    if sony || pad.vendor_id() == Some(SONY) { Layout::PlayStation } else { Layout::Xbox }
}

fn repeats(cmd: Command) -> bool {
    matches!(
        cmd,
        Command::Up | Command::Down | Command::SeekBack | Command::SeekForward | Command::VolumeDown | Command::VolumeUp
    )
}

fn keyboard(ctx: &Context) -> Vec<Command> {
    const KEYS: [(Key, Command); 19] = [
        (Key::ArrowUp, Command::Up),
        (Key::ArrowDown, Command::Down),
        (Key::PageUp, Command::PageUp),
        (Key::PageDown, Command::PageDown),
        (Key::ArrowLeft, Command::SeekBack),
        (Key::ArrowRight, Command::SeekForward),
        (Key::Enter, Command::Confirm),
        (Key::Escape, Command::Back),
        (Key::Backspace, Command::Back),
        (Key::Space, Command::PlayPause),
        (Key::OpenBracket, Command::Previous),
        (Key::CloseBracket, Command::Next),
        (Key::Minus, Command::VolumeDown),
        (Key::Plus, Command::VolumeUp),
        (Key::S, Command::Shuffle),
        (Key::R, Command::Repeat),
        (Key::F, Command::ChooseFolder),
        (Key::O, Command::Settings),
        (Key::M, Command::Minimize),
    ];
    ctx.input(|i| {
        let mut out: Vec<Command> = KEYS.iter().filter(|(key, _)| i.key_pressed(*key)).map(|(_, cmd)| *cmd).collect();
        if i.key_pressed(Key::Equals) {
            out.push(Command::VolumeUp);
        }
        out
    })
}
