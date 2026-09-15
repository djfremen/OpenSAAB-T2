"""Explicit Android package targets; shared emulator sources, isolated artifacts."""
from dataclasses import dataclass


@dataclass(frozen=True)
class Profile:
    abi: str
    rust_target: str
    elf_class: int
    elf_machine: int
    package: str
    label: str


PROFILES = {
    'arm64': Profile('arm64-v8a', 'aarch64-linux-android', 2, 183,
                     'com.opensaab.tech2', 'OpenSAAB T2'),
    'headunit-arm32': Profile('armeabi-v7a', 'armv7-linux-androideabi', 1, 40,
                             'com.opensaab.tech2.headunit32', 'OpenSAAB T2 Head Unit 32-bit'),
}


def validate_elf(header, profile):
    """Check actual ELF class/machine, not a filename or directory label."""
    if (len(header) < 20 or header[:4] != b'\x7fELF' or header[4] != profile.elf_class
            or header[5] != 1 or int.from_bytes(header[18:20], 'little') != profile.elf_machine):
        raise ValueError(f'Native executable does not match {profile.abi}')
