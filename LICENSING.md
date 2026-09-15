# OpenSAAB T2 licensing

Selected September 15, 2026 (UTC), with the project owner's authorization.

Original OpenSAAB source code is licensed under the **Mozilla Public License,
version 2.0 (MPL-2.0)**. The complete, unmodified license is in [LICENSE](LICENSE).

This Source Code Form is subject to the terms of the Mozilla Public License,
v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain
one at https://mozilla.org/MPL/2.0/.

## Scope

The notice applies to original OpenSAAB Rust source in `src/`, Java source in
`android/`, original tests, build/configuration scripts, and accompanying original
software documentation in this repository, unless a file carries a different
license or is identified below as third-party material. Existing copyright and
license notices take precedence for their respective material and must be retained.
Research quotations, copied documents and third-party code are not relicensed.

The project license does **not** license GM, Saab, Vetronix/Bosch or adapter-vendor
firmware or software. In particular, `eprom.bin`, `opsys.dwn`, `candi.bin`, Saab
PCMCIA images, native Tech2Win executables, installer files and vendor DLLs retain
their owners' terms. The owner's requested three-file APK packaging profile does
not itself establish redistribution permission. An emulator's license cannot
grant rights in the guest software it executes.

Dependencies retain their own licenses. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
Brand names and logos are not granted trademark rights by MPL-2.0. The approved
logo and usage guidance remain in [assets/branding/README.md](assets/branding/README.md).
Do not represent a fork as an official OpenSAAB release.

## Why this license

MPL-2.0 is a practical fit for a community emulator with Android, desktop and
possible future iOS clients and different adapter integrations. When distributing
software based on covered files, the covered source and modifications must be
available to recipients under MPL-2.0. Separate files that contain no MPL-covered
code can use another license. This preserves access to improvements to our code
without requiring every independent adapter component or platform integration
to adopt the same license. It does not guarantee approval by any app store.

People may use, modify, redistribute and sell the software. Donations, sponsorship,
paid support and commercial services are allowed. Contributions are voluntary:
recipients are entitled to the required source, but nobody is required to submit
a pull request, contribute upstream, donate, or make private modifications public.
The license does not guarantee that all new features in a fork will be open source.

Contributors retain their copyright and submit original contributions under
MPL-2.0; see [CONTRIBUTING.md](CONTRIBUTING.md). A later proprietary relicense is
not automatically available for other people's contributions, and changing future
terms cannot withdraw recipients' existing license rights.

## Release responsibilities

For each APK, make the corresponding MPL-covered source available and tell users
where to find it. Publish a matching source tag/archive, license, dependency
notices, build instructions, APK hash and signing-certificate identity. Do not
claim a dirty working tree matches its base commit alone. Source availability is
not proof of independently reproducible builds.

This license applies to this source release. It does not clear third-party rights
or change the license of the separately hosted OpenSAAB API or other repositories.

Official references: [Mozilla MPL FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/)
and [MPL-2.0 text](https://www.mozilla.org/en-US/MPL/2.0/).
