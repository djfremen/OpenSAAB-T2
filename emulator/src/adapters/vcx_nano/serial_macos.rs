// SPDX-License-Identifier: MPL-2.0
//! Native macOS CDC binding. Adapter startup and CAN decoding stay in shared Rust.
use super::channel::UsbTransport;
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, OpenOptionsExt},
    },
    path::Path,
    time::{Duration, Instant},
};

const IOSSIOSPEED: libc::c_ulong = 0x80045402;
const MODEM_BITS: libc::c_int = libc::TIOCM_DTR | libc::TIOCM_RTS;

pub struct Port {
    file: Option<File>,
    original: Option<libc::termios>,
    original_modem: Option<libc::c_int>,
}
impl Port {
    pub fn open(path: &Path) -> io::Result<Self> {
        if path.parent() != Some(Path::new("/dev"))
            || !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("cu.usbmodem"))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Select an explicit /dev/cu.usbmodem... Nano port",
            ));
        }
        let mut port = Self::open_raw(path)?;
        let fd = port.file.as_ref().unwrap().as_raw_fd();
        let mut modem = 0;
        if unsafe { libc::ioctl(fd, libc::TIOCMGET as libc::c_ulong, &mut modem) } != 0 {
            return Err(io::Error::last_os_error());
        }
        port.original_modem = Some(modem & MODEM_BITS);
        let mut baud: libc::speed_t = 921_600;
        // SAFETY: owned descriptor, initialized speed_t and ioctl-specific pointers.
        if unsafe { libc::ioctl(fd, IOSSIOSPEED, &mut baud) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut bits = MODEM_BITS;
        if unsafe { libc::ioctl(fd, libc::TIOCMBIS as libc::c_ulong, &mut bits) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(port)
    }
    fn open_raw(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(path)?;
        if !file.metadata()?.file_type().is_char_device() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Nano serial path is not a character device",
            ));
        }
        let fd = file.as_raw_fd();
        // SAFETY: all syscalls below use the owned live descriptor and initialized storage.
        if unsafe { libc::ioctl(fd, libc::TIOCEXCL as libc::c_ulong) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // Build the guard before mutating settings; failed setup also releases ownership.
        let mut port = Self {
            file: Some(file),
            original: None,
            original_modem: None,
        };
        let mut settings = std::mem::MaybeUninit::<libc::termios>::uninit();
        if unsafe { libc::tcgetattr(fd, settings.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut raw = unsafe { settings.assume_init() };
        port.original = Some(raw);
        unsafe { libc::cfmakeraw(&mut raw) };
        raw.c_cflag &= !(libc::CSIZE | libc::PARENB | libc::CSTOPB | libc::CRTSCTS | libc::HUPCL);
        raw.c_cflag |= libc::CS8 | libc::CLOCAL | libc::CREAD;
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(port)
    }
    fn wait(&self, events: libc::c_short, timeout: Duration) -> io::Result<bool> {
        let file = self
            .file
            .as_ref()
            .ok_or_else(|| io::Error::other("Nano serial port closed"))?;
        let end = Instant::now() + timeout;
        loop {
            let remaining = end.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(false);
            }
            let mut poll = libc::pollfd {
                fd: file.as_raw_fd(),
                events,
                revents: 0,
            };
            // SAFETY: poll points to one valid pollfd and has a bounded timeout.
            let rc =
                unsafe { libc::poll(&mut poll, 1, remaining.as_millis().max(1) as libc::c_int) };
            if rc < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err(io::Error::other("Nano serial device disconnected"));
            }
            return Ok(rc > 0 && poll.revents & events != 0);
        }
    }
    pub fn close(&mut self) -> io::Result<()> {
        let Some(file) = self.file.take() else {
            return Ok(());
        };
        let fd = file.as_raw_fd();
        let mut first = None;
        let mut check = |rc| {
            if rc != 0 && first.is_none() {
                first = Some(io::Error::last_os_error());
            }
        };
        // Restore both line and modem settings, release exclusivity, then close,
        // even if unplug means a restoration syscall fails.
        unsafe {
            if let Some(original) = self.original.take() {
                check(libc::tcsetattr(fd, libc::TCSANOW, &original));
            }
            if let Some(mut original) = self.original_modem.take() {
                let mut clear = MODEM_BITS & !original;
                check(libc::ioctl(fd, libc::TIOCMBIC as libc::c_ulong, &mut clear));
                check(libc::ioctl(
                    fd,
                    libc::TIOCMBIS as libc::c_ulong,
                    &mut original,
                ));
            }
            check(libc::ioctl(fd, libc::TIOCNXCL as libc::c_ulong));
        }
        drop(file);
        first.map_or(Ok(()), Err)
    }
}
impl UsbTransport for Port {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty() || bytes.len() > 8194 {
            return Err("Nano serial TX frame size invalid".into());
        }
        let end = Instant::now() + Duration::from_millis(500);
        loop {
            if !self
                .wait(libc::POLLOUT, end.saturating_duration_since(Instant::now()))
                .map_err(|e| e.to_string())?
            {
                return Err("Nano serial write deadline expired".into());
            }
            match self.file.as_mut().unwrap().write(bytes) {
                Ok(n) if n == bytes.len() => return Ok(()),
                Ok(n) => {
                    return Err(format!(
                        "Nano serial partial write: {n}/{}; outcome unknown",
                        bytes.len()
                    ))
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }
    fn read(&mut self) -> Result<Vec<u8>, String> {
        if !self
            .wait(libc::POLLIN, Duration::from_millis(20))
            .map_err(|e| e.to_string())?
        {
            return Ok(vec![]);
        }
        let mut bytes = [0; 4096];
        match self.file.as_mut().unwrap().read(&mut bytes) {
            Ok(0) => Err("Nano serial device disconnected".into()),
            Ok(n) => Ok(bytes[..n].to_vec()),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) =>
            {
                Ok(vec![])
            }
            Err(e) => Err(e.to_string()),
        }
    }
}
impl Drop for Port {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::{FromRawFd, OwnedFd};
    #[test]
    fn native_fragmented_exchange_restores_settings_and_releases_ownership() {
        let (mut master, mut slave) = (-1, -1);
        let mut name = [0i8; 256];
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    name.as_mut_ptr(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            },
            0
        );
        let mut master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        let path = Path::new(
            unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }
                .to_str()
                .unwrap(),
        );
        let mut before = std::mem::MaybeUninit::<libc::termios>::uninit();
        assert_eq!(
            unsafe { libc::tcgetattr(slave.as_raw_fd(), before.as_mut_ptr()) },
            0
        );
        let before = unsafe { before.assume_init() };
        // A PTY cannot set the hardware baud; production Port::open also applies
        // IOSSIOSPEED and DTR/RTS, verified separately against the physical Nano.
        let port = Port::open_raw(path).unwrap();
        let request = super::super::protocol::Frame::command(0x80, b"\0TEST");
        let wire = request.encode().unwrap();
        let peer = std::thread::spawn(move || {
            assert_eq!(
                unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
                0
            );
            let end = Instant::now() + Duration::from_secs(2);
            let mut got = Vec::new();
            while got.len() < wire.len() {
                assert!(Instant::now() < end, "PTY peer read deadline");
                let mut chunk = [0; 64];
                match master.read(&mut chunk) {
                    Ok(0) => panic!("PTY peer disconnected"),
                    Ok(n) => got.extend_from_slice(&chunk[..n]),
                    Err(e)
                        if matches!(
                            e.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        std::thread::sleep(Duration::from_millis(1))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
            assert_eq!(got, wire);
            master.write_all(&wire[..3]).unwrap();
            std::thread::sleep(Duration::from_millis(10));
            master.write_all(&wire[3..]).unwrap();
            master
        });
        let mut client = super::super::channel::Client::new(port);
        assert_eq!(client.exchange(&request, &mut |_| Ok(())).unwrap(), request);
        let _master = peer.join().unwrap();
        client.transport_mut().close().unwrap();
        let mut after = before;
        assert_eq!(unsafe { libc::tcgetattr(slave.as_raw_fd(), &mut after) }, 0);
        assert_eq!(before.c_iflag, after.c_iflag);
        assert_eq!(before.c_oflag, after.c_oflag);
        assert_eq!(before.c_cflag, after.c_cflag);
        assert_eq!(
            before.c_lflag & !libc::PENDIN,
            after.c_lflag & !libc::PENDIN
        );
        assert_eq!(before.c_ispeed, after.c_ispeed);
        assert_eq!(before.c_ospeed, after.c_ospeed);
        assert!(OpenOptions::new().read(true).write(true).open(path).is_ok());
    }
    #[test]
    fn rejects_non_cdc_paths_before_opening() {
        for name in [
            "/tmp/cu.usbmodem1",
            "cu.usbmodem1",
            "/dev/cu.Bluetooth-Incoming-Port",
            "/dev/cu.usbmodem1/../tty",
        ] {
            assert!(Port::open(Path::new(name)).is_err(), "{name}");
        }
    }
}
