// SPDX-License-Identifier: MPL-2.0
//! macOS USB CDC callout ports, using the OS serial driver and native syscalls.
//! A usbmodem name identifies a candidate only, not the Chipsoft brand.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub fn candidates() -> io::Result<Vec<PathBuf>> {
    let mut ports = Vec::new();
    for entry in std::fs::read_dir("/dev")? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("cu.usbmodem")
            && entry.file_type()?.is_char_device()
        {
            ports.push(entry.path());
        }
    }
    ports.sort();
    Ok(ports)
}

pub struct Port {
    file: File,
    original: Option<libc::termios>,
}

impl Port {
    /// Only the USB CDC callout namespace is accepted. No automatic probing of
    /// every serial device, no arbitrary path and no symlink traversal.
    pub fn open(path: &Path) -> io::Result<Self> {
        if path.parent() != Some(Path::new("/dev"))
            || !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("cu.usbmodem"))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "select a USB CDC port listed by chipsoft-probe --list (/dev/cu.usbmodem...)",
            ));
        }
        Self::open_serial(path)
    }

    fn open_serial(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(path)?;
        if !file.metadata()?.file_type().is_char_device() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "serial path is not a character device",
            ));
        }
        let fd = file.as_raw_fd();
        // SAFETY: fd is owned by file and remains open throughout these calls.
        if unsafe { libc::ioctl(fd, libc::TIOCEXCL as libc::c_ulong) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut port = Self {
            file,
            original: None,
        };
        // The guard now releases exclusivity if any later configuration fails.
        let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
        // SAFETY: writable storage sized for termios; assumed initialized only
        // after tcgetattr reports success.
        if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let original = unsafe { original.assume_init() };
        let mut raw = original;
        port.original = Some(original);
        // SAFETY: raw is a fully initialized termios value. cfmakeraw changes
        // line processing but preserves input/output speed. Never call cfsetspeed.
        unsafe { libc::cfmakeraw(&mut raw) };
        raw.c_cflag |= libc::CLOCAL | libc::CREAD;
        raw.c_cflag &= !(libc::CRTSCTS | libc::HUPCL);
        // O_NONBLOCK controls waiting; VMIN=1 means an empty connected port
        // returns WouldBlock, so a zero-byte read can signal disconnect.
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        // TCSANOW deliberately avoids waiting for queued output to drain.
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::tcflush(fd, libc::TCIOFLUSH) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(port)
    }
}

impl Read for Port {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.file.read(out)
    }
}
impl Write for Port {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.file.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        // File::flush does not wait for serial TX. Callers must use protocol
        // replies/deadlines, never tcdrain, to establish completion.
        self.file.flush()
    }
}
impl Drop for Port {
    fn drop(&mut self) {
        let fd = self.file.as_raw_fd();
        // SAFETY: owned file is still open; restore without waiting for output.
        // Cleanup is best effort after unplug. File's Drop closes the fd.
        unsafe {
            if let Some(original) = self.original.as_ref() {
                libc::tcsetattr(fd, libc::TCSANOW, original);
            }
            libc::ioctl(fd, libc::TIOCNXCL as libc::c_ulong);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    fn pty() -> (File, OwnedFd, PathBuf) {
        let mut master = -1;
        let mut slave = -1;
        let mut name = [0i8; 256];
        // SAFETY: valid output pointers; null termios/winsize selects defaults.
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
        // SAFETY: successful openpty returns distinct owned descriptors and a
        // NUL-terminated device name in the supplied buffer.
        unsafe {
            let path = PathBuf::from(std::ffi::CStr::from_ptr(name.as_ptr()).to_str().unwrap());
            (File::from_raw_fd(master), OwnedFd::from_raw_fd(slave), path)
        }
    }

    #[test]
    fn native_pty_probe_preserves_baud_and_restores_settings() {
        let (mut master, slave, path) = pty();
        let mut before = std::mem::MaybeUninit::<libc::termios>::uninit();
        assert_eq!(
            unsafe { libc::tcgetattr(slave.as_raw_fd(), before.as_mut_ptr()) },
            0
        );
        let before = unsafe { before.assume_init() };
        let mut port = Port::open_serial(&path).unwrap();
        let mut active = before;
        assert_eq!(
            unsafe { libc::tcgetattr(slave.as_raw_fd(), &mut active) },
            0
        );
        assert_eq!(active.c_ispeed, before.c_ispeed);
        assert_eq!(active.c_ospeed, before.c_ospeed);
        assert_eq!(active.c_lflag & (libc::ICANON | libc::ECHO), 0);
        let flags = unsafe { libc::fcntl(port.file.as_raw_fd(), libc::F_GETFL) };
        assert_ne!(flags & libc::O_NONBLOCK, 0);
        let mut byte = [0];
        assert_eq!(
            port.read(&mut byte).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        // Bounded peer test: no blocking Read waiting indefinitely for a bug.
        let peer = std::thread::spawn(move || {
            assert_eq!(
                unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
                0
            );
            let mut request = Vec::new();
            let start = std::time::Instant::now();
            while request.len() < 8 {
                assert!(start.elapsed() < Duration::from_secs(2));
                let mut chunk = [0; 8];
                match master.read(&mut chunk) {
                    Ok(n) => request.extend_from_slice(&chunk[..n]),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
            assert_eq!(request, [1, 0, 0, 0, 0, 0, 0, 0]);
            let reply = b"\x01\x00\x1b\x00\x00\x00\xc1\x06CHIPSOFT J2534 Pro v. 1.5.2";
            master.write_all(&reply[..1]).unwrap();
            std::thread::sleep(Duration::from_millis(5));
            master.write_all(&reply[1..]).unwrap();
            master // keep master open until identification and cleanup complete
        });
        assert_eq!(
            crate::chipsoft_probe::identify(
                &mut port,
                Duration::from_secs(2),
                &AtomicBool::new(false),
                |_| {}
            )
            .unwrap(),
            "CHIPSOFT J2534 Pro v. 1.5.2"
        );
        let _master = peer.join().unwrap();
        drop(port);
        let mut after = before;
        assert_eq!(unsafe { libc::tcgetattr(slave.as_raw_fd(), &mut after) }, 0);
        // macOS sets PENDIN (runtime input state) when restoring canonical
        // mode. Compare configuration flags without that kernel state bit.
        assert_eq!(
            after.c_lflag & !libc::PENDIN,
            before.c_lflag & !libc::PENDIN
        );
        assert_eq!(after.c_cflag, before.c_cflag);
        assert_eq!(after.c_iflag, before.c_iflag);
        assert_eq!(after.c_oflag, before.c_oflag);
        assert_eq!(after.c_cc, before.c_cc);
        assert!(OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .is_ok());
    }

    #[test]
    fn public_open_rejects_non_cdc_paths() {
        for path in [
            "/dev/cu.Bluetooth-Incoming-Port",
            "/tmp/cu.usbmodem1",
            "cu.usbmodem1",
            "/dev/cu.usbmodem1/../tty",
        ] {
            assert!(Port::open(Path::new(path)).is_err());
        }
    }
}
