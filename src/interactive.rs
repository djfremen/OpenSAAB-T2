// SPDX-License-Identifier: MPL-2.0
//! Offline interactive frontend protocol shared by Android and headless operators.
//! The frontend displays guest VRAM and submits real encoder keys. It does not
//! synthesize menus or diagnostic results. One atomic mailbox, one producer.
use crate::{artifacts, bus::Tech2Bus, recovery};
use m68k::CpuCore;
use std::{
    fs, io,
    path::Path,
    time::{Duration, Instant},
};

pub enum Poll {
    Continue,
    Paused,
    Stop,
}
#[derive(Debug, PartialEq)]
enum Command {
    Enter,
    Key(u8),
    Stop,
}
fn parse(text: &str) -> io::Result<Command> {
    let text = text.trim();
    match text {
        "enter" => Ok(Command::Enter),
        "stop" => Ok(Command::Stop),
        _ => text
            .strip_prefix("0x")
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .filter(|n| *n <= 31)
            .map(Command::Key)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "expected enter, stop, or one 0x00..0x1f encoder code",
                )
            }),
    }
}

// Arrow navigation cannot enter an operation. Capture immediately before an
// action instead of copying the entire guest/card for every highlighted row.
fn needs_checkpoint(command: &Command) -> bool {
    !matches!(
        command,
        Command::Stop | Command::Key(crate::bus::KEY_UP | crate::bus::KEY_DOWN)
    )
}

pub struct Session<'a> {
    dir: &'a Path,
    started: Instant,
    next: Instant,
    next_frame: Instant,
    next_limits: Instant,
    screen: String,
    frame: artifacts::LiveFrame,
    checkpoint: Option<recovery::Checkpoint>,
    waiting: Option<Instant>,
}
impl<'a> Session<'a> {
    pub fn new(dir: &'a Path) -> Self {
        Self {
            dir,
            started: Instant::now(),
            next: Instant::now(),
            next_frame: Instant::now(),
            next_limits: Instant::now(),
            screen: String::new(),
            frame: artifacts::LiveFrame::default(),
            checkpoint: None,
            waiting: None,
        }
    }
    pub fn poll(&mut self, cpu: &mut CpuCore, bus: &mut Tech2Bus) -> io::Result<Poll> {
        let now = Instant::now();
        if now.duration_since(self.started) >= Duration::from_secs(1800) {
            println!("SESSION: 30-minute deadline reached; restart to continue");
            return Ok(Poll::Stop);
        }
        if now < self.next {
            return Ok(if self.waiting.is_some() {
                Poll::Paused
            } else {
                Poll::Continue
            });
        }
        self.next = now + Duration::from_millis(10);
        if now >= self.next_limits {
            self.next_limits = now + Duration::from_secs(1);
            for name in ["tech2.log", "trace.jsonl"] {
                if fs::metadata(self.dir.join(name)).is_ok_and(|m| m.len() > 16 * 1024 * 1024) {
                    println!("SESSION: Log size limit reached; restart to continue");
                    return Ok(Poll::Stop);
                }
            }
        }
        // Input is polled independently from rendering and log housekeeping.
        if now >= self.next_frame {
            self.next_frame = now + Duration::from_millis(33);
            // Publish by rename so a reader never sees a partially written PPM.
            self.frame.publish(bus, self.dir)?;
            let screen = bus.screen_text();
            if screen != self.screen {
                fs::write(self.dir.join("screen.txt.tmp"), &screen)?;
                fs::rename(self.dir.join("screen.txt.tmp"), self.dir.join("screen.txt"))?;
                println!(
                    "LCD: {}",
                    screen.split_whitespace().collect::<Vec<_>>().join(" ")
                );
                self.screen = screen;
            }
            if self.waiting.is_none() {
                if let Some(reason) = recovery::unavailable_reason(&self.screen) {
                    println!("SESSION: {reason} Returning to previous menu in 5 seconds.");
                    self.waiting = Some(now);
                }
            }
        }
        let path = self.dir.join("interactive-key.txt");
        let command = match fs::metadata(&path) {
            Ok(meta) => {
                if meta.len() > 32 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "key mailbox exceeds 32 bytes",
                    ));
                }
                let text = fs::read_to_string(&path)?;
                let command = parse(&text)?;
                // Preserve the mailbox until the prior down/up pair is consumed.
                // STOP and paused recovery must remain responsive regardless of guest state.
                if command != Command::Stop
                    && self.waiting.is_none()
                    && (!bus.key_queue.is_empty() || bus.key_active.is_some())
                {
                    return Ok(Poll::Continue);
                }
                fs::remove_file(path)?;
                Some(command)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        if command == Some(Command::Stop) {
            return Ok(Poll::Stop);
        }
        if let Some(started) = self.waiting {
            if command == Some(Command::Key(crate::bus::KEY_EXIT))
                || now.duration_since(started) >= Duration::from_secs(5)
            {
                if let Some(saved) = self.checkpoint.take() {
                    saved.restore(cpu, bus);
                    self.waiting = None;
                    self.screen.clear();
                    println!("SESSION: Operation cancelled; restored previous guest menu (offline recovery)");
                } else {
                    println!("SESSION: No prior menu checkpoint; restart required");
                    return Ok(Poll::Stop);
                }
            }
            return Ok(Poll::Paused);
        }
        if let Some(command) = command {
            if needs_checkpoint(&command) && recovery::can_checkpoint(bus) {
                self.checkpoint =
                    Some(recovery::Checkpoint::capture(cpu, bus).map_err(io::Error::other)?);
            }
            let code = match command {
                Command::Enter if bus.guest_splash_reached() => crate::bus::KEY_ENTER_DEFAULT,
                Command::Enter => crate::bus::KEY_SELECT,
                Command::Key(code) => code,
                Command::Stop => unreachable!(),
            };
            println!(
                "KEYPAD: encoder={code:#04x} source=interactive-frontend wall_ms={}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            );
            bus.trace_event("native_operator_key", &format!("encoder={code:#04x}"));
            if code == crate::bus::KEY_EXIT {
                bus.exit_key();
            } else {
                bus.press_key(code);
            }
        }
        Ok(Poll::Continue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn busy_guest_keeps_mailbox_and_stop_bypasses_backpressure() {
        let dir = std::env::temp_dir().join(format!("opensaab-input-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mailbox = dir.join("interactive-key.txt");
        let mut bus = Tech2Bus::new(
            vec![0; 0x40000],
            vec![],
            crate::bus::ExecutionMode::Fidelity,
        );
        let mut cpu = CpuCore::new();
        bus.press_key(crate::bus::KEY_DOWN);
        let depth = bus.key_queue.len();
        let mut session = Session::new(&dir);
        session.next_frame = Instant::now() + Duration::from_secs(10);
        session.next_limits = session.next_frame;
        fs::write(&mailbox, "0x09\n").unwrap();
        assert!(matches!(
            session.poll(&mut cpu, &mut bus).unwrap(),
            Poll::Continue
        ));
        assert!(mailbox.exists());
        assert_eq!(bus.key_queue.len(), depth);
        fs::write(&mailbox, "stop\n").unwrap();
        session.next = Instant::now();
        assert!(matches!(
            session.poll(&mut cpu, &mut bus).unwrap(),
            Poll::Stop
        ));
        assert!(!mailbox.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn navigation_does_not_replace_recovery_but_actions_do() {
        for code in 0..32 {
            assert_eq!(
                needs_checkpoint(&Command::Key(code)),
                code != crate::bus::KEY_UP && code != crate::bus::KEY_DOWN
            );
        }
        assert!(needs_checkpoint(&Command::Enter));
        assert!(!needs_checkpoint(&Command::Stop));
    }
    #[test]
    fn mailbox_rejects_multiple_keys_and_invalid_encoder_values() {
        assert_eq!(parse("enter\n").unwrap(), Command::Enter);
        assert_eq!(parse("0x1f").unwrap(), Command::Key(31));
        assert_eq!(parse("stop").unwrap(), Command::Stop);
        for text in ["0x20", "0xff", "0x10\n0x01", "", "shell", "16"] {
            assert!(parse(text).is_err(), "{text:?}");
        }
    }
}
