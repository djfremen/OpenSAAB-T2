// SPDX-License-Identifier: MPL-2.0
//! Release temporary usbfs claims without Android's automatic kernel reconnect.
//! The caller retains ownership of the Android-granted file descriptor.

const SET_CONFIGURATION: libc::c_ulong = 0x8004_5505;
const RELEASE_INTERFACE: libc::c_ulong = 0x8004_5510;

fn operation(fd: i32, request: libc::c_ulong, value: u32) -> i32 {
    let mut argument = value;
    // SAFETY: the ioctl accepts a pointer to one unsigned 32-bit integer. It
    // neither retains this pointer nor closes the borrowed descriptor.
    let result = unsafe { libc::ioctl(fd, request as _, &mut argument) };
    if result < 0 {
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO)
    } else {
        0
    }
}

pub fn select(fd: i32, configuration: i32) -> i32 {
    if !(1..=255).contains(&configuration) {
        return libc::EINVAL;
    }
    operation(fd, SET_CONFIGURATION, configuration as u32)
}

pub fn release(fd: i32, interface: i32) -> i32 {
    if !(0..=255).contains(&interface) {
        return libc::EINVAL;
    }
    // Deliberately no USBDEVFS_CONNECT: it would rebind the driver and make the
    // subsequent SET_CONFIGURATION busy again. Normal final release is unchanged.
    operation(fd, RELEASE_INTERFACE, interface as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_descriptor_returns_the_real_errno() {
        assert_eq!(select(-1, 2), libc::EBADF);
        assert_eq!(release(-1, 0), libc::EBADF);
    }

    #[test]
    fn invalid_ids_do_not_issue_an_ioctl() {
        for id in [-1, 0, 256] {
            assert_eq!(select(-1, id), libc::EINVAL);
        }
        for id in [-1, 256] {
            assert_eq!(release(-1, id), libc::EINVAL);
        }
    }
}
