// SPDX-License-Identifier: MPL-2.0
//! Validated command-line options. Explicit values never silently fall back.
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarnessTarget {
    Menus,
    NativeManual,
    Recovery,
    Ecm,
    EcmLink,
    EcmLink1367,
    SecurityLink1367,
    DtcLink1367,
    EngineData1367,
}

#[derive(Debug)]
pub struct Options {
    pub headless: bool,
    pub interactive_headless: bool,
    pub verbose: bool,
    pub trace_calls: bool,
    pub test_harness: bool,
    pub research: bool,
    pub fast_boot: bool,
    pub mock_vehicle: bool,
    pub strict: bool,
    pub candi: bool,
    pub candi_native_link: bool,
    pub candi_on_demand: bool,
    pub candi_nano_ssh: Option<String>,
    pub candi_seatbelt_audible: bool,
    pub candi_j2534_adapter: tech2_emu::vcx::J2534Adapter,
    pub candi_nano_usb_token: Option<String>,
    pub candi_chipsoft_usb_token: Option<String>,
    pub candi_chipsoft_symbol_only: bool,
    pub candi_chipsoft_seeds: bool,
    pub candi_chipsoft_audible: bool,
    pub candi_nano_clear_dtc: bool,
    pub candi_nano_control: Option<PathBuf>,
    pub candi_firmware: Option<PathBuf>,
    pub vcx_ssh: Option<String>,
    pub vcx_control: Option<PathBuf>,
    pub vcx_ecu: tech2_emu::vcx::EcuProfile,
    pub ecm_information: bool,
    pub ibus_dtc: Option<tech2_emu::ibus_dtc::Module>,
    pub help: bool,
    pub replay: Option<PathBuf>,
    pub card: Option<PathBuf>,
    pub boot: Option<PathBuf>,
    pub opsys: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub output: PathBuf,
    pub max_insns: u64,
    pub warmup_insns: u64,
    pub target: HarnessTarget,
}

pub fn parse_bool(name: &str, value: Option<String>) -> Result<bool, String> {
    match value.as_deref().map(str::to_ascii_lowercase).as_deref() {
        None | Some("0" | "false" | "no" | "off") => Ok(false),
        Some("" | "1" | "true" | "yes" | "on") => Ok(true),
        Some(v) => Err(format!("{name}: invalid boolean {v:?}; use 0 or 1")),
    }
}

pub fn env_flag(name: &str) -> bool {
    // Runtime debug flags use the same false-value semantics as the CLI.
    parse_bool(name, std::env::var(name).ok()).unwrap_or(false)
}

fn count(name: &str, value: &str, allow_zero: bool) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|v| allow_zero || *v > 0)
        .ok_or_else(|| {
            format!(
                "{name}: expected {}integer, got {value:?}",
                if allow_zero {
                    "a nonnegative "
                } else {
                    "a positive "
                }
            )
        })
}

impl Options {
    /// Local guest flash semantics are independent of vehicle-command permissions.
    /// Native manual diagnostics must also initialize/read back a cleared SSA card.
    pub fn uses_ssa_flash(&self) -> bool {
        self.target == HarnessTarget::SecurityLink1367
            || self.candi_chipsoft_seeds
            || (self.target == HarnessTarget::NativeManual && self.candi_native_link)
    }

    pub fn parse(
        args: impl IntoIterator<Item = String>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, String> {
        let mut out = Self {
            headless: parse_bool("HEADLESS", env("HEADLESS"))? || !cfg!(feature = "gui"),
            interactive_headless: false,
            verbose: parse_bool("TECH2_VERBOSE", env("TECH2_VERBOSE"))?,
            trace_calls: false,
            test_harness: parse_bool("TEST_HARNESS", env("TEST_HARNESS"))?,
            research: parse_bool("TECH2_RESEARCH_HARNESS", env("TECH2_RESEARCH_HARNESS"))?
                || parse_bool("HARNESS_PLANTS", env("HARNESS_PLANTS"))?,
            fast_boot: parse_bool("TECH2_FAKE_POST", env("TECH2_FAKE_POST"))?,
            mock_vehicle: parse_bool("TECH2_MOCK_VEHICLE", env("TECH2_MOCK_VEHICLE"))?,
            strict: parse_bool("STRICT_BOOT", env("STRICT_BOOT"))?
                || parse_bool("STRICT_GUEST_BOOT", env("STRICT_GUEST_BOOT"))?,
            candi_native_link: false,
            candi_on_demand: false,
            candi_nano_ssh: None,
            candi_seatbelt_audible: false,
            candi_j2534_adapter: tech2_emu::vcx::J2534Adapter::default(),
            candi_nano_usb_token: None,
            candi_chipsoft_usb_token: None,
            candi_chipsoft_symbol_only: false,
            candi_chipsoft_seeds: false,
            candi_chipsoft_audible: false,
            candi_nano_clear_dtc: false,
            candi_nano_control: None,
            candi: parse_bool("TECH2_CANDI", env("TECH2_CANDI"))?,
            candi_firmware: env("TECH2_CANDI_FIRMWARE")
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
            help: false,
            vcx_ssh: None,
            vcx_control: None,
            vcx_ecu: tech2_emu::vcx::EcuProfile::default(),
            ecm_information: false,
            ibus_dtc: None,
            replay: None,
            card: None,
            boot: None,
            opsys: None,
            config: None,
            output: PathBuf::from("."),
            max_insns: crate::bus::MAX_INSNS_DEFAULT,
            warmup_insns: 30_000_000,
            target: HarnessTarget::Menus,
        };
        if let Some(v) = env("MAX_INSNS") {
            out.max_insns = count("MAX_INSNS", &v, false)?;
        }
        if let Some(v) = env("WARMUP_INSNS") {
            out.warmup_insns = count("WARMUP_INSNS", &v, true)?;
        }
        let mut args = args.into_iter();
        let mut positional = false;
        let mut target_set = false;
        while let Some(arg) = args.next() {
            if !positional {
                match arg.as_str() {
                    "--" => { positional = true; continue; }
                    "--headless" | "-h" => { out.headless = true; continue; }
                    "--interactive-headless" => { out.interactive_headless = true; out.headless = true; continue; }
                    "--verbose" | "-v" => { out.verbose = true; continue; }
                    "--trace-calls" => { out.trace_calls = true; out.verbose = true; continue; }
                    "--test-harness" => { out.test_harness = true; continue; }
                    "--research-harness" => { out.research = true; continue; }
                    "--fast-boot" | "--fake-post" => { out.fast_boot = true; continue; }
                    "--mock-vehicle" => { out.mock_vehicle = true; continue; }
                    "--strict" => { out.strict = true; continue; }
                    "--candi-on-demand" => { out.candi_on_demand = true; continue; }
                    "--candi-native-link" => { out.candi_native_link = true; out.candi = true; continue; }
                    "--candi-nano-clear-dtc" => { out.candi_nano_clear_dtc = true; continue; }
                    "--candi-chipsoft-audible" => {out.candi_chipsoft_audible=true;continue;}
                    "--candi-chipsoft-seeds" => {out.candi_chipsoft_seeds=true;continue;}
                    "--candi-chipsoft-symbol-only" => {out.candi_chipsoft_symbol_only=true;continue;}
                    "--candi-seatbelt-audible" => { out.candi_seatbelt_audible = true; continue; }
                    "--candi" => { out.candi = true; continue; }
                    "--ecm-information" => { out.ecm_information = true; continue; }
                    "--help" | "-help" => { out.help = true; continue; }
                    "--candi-chipsoft-usb-token" | "--candi-j2534-adapter" | "--candi-nano-usb-token" | "--candi-nano-ssh" | "--candi-nano-control" | "--ibus-dtc" | "--vcx-ecu" | "--vcx-ssh" | "--vcx-ssh-control" | "--candi-firmware" | "--replay-keys" | "--boot" | "--opsys" | "--config" | "--output-dir" | "--max-insns" | "--warmup-insns" | "--harness-target" => {
                        let value = args.next().filter(|v| !v.is_empty() && !v.starts_with("--"))
                            .ok_or_else(|| format!("{arg} requires a value"))?;
                        match arg.as_str() {
                            "--candi-j2534-adapter" => out.candi_j2534_adapter = tech2_emu::vcx::J2534Adapter::parse(&value)?,
                            "--candi-nano-ssh" => out.candi_nano_ssh = Some(value),
                            "--candi-nano-usb-token" => out.candi_nano_usb_token = Some(value),
                            "--candi-chipsoft-usb-token" => out.candi_chipsoft_usb_token = Some(value),
                            "--candi-nano-control" => out.candi_nano_control = Some(value.into()),
                            "--ibus-dtc" => out.ibus_dtc = Some(tech2_emu::ibus_dtc::Module::parse(&value)?),
                            "--candi-firmware" => out.candi_firmware = Some(value.into()),
                            "--vcx-ssh" => out.vcx_ssh = Some(value),
                            "--vcx-ecu" => out.vcx_ecu = tech2_emu::vcx::EcuProfile::parse(&value)?,
                            "--vcx-ssh-control" => out.vcx_control = Some(value.into()),
                            "--replay-keys" => out.replay = Some(value.into()),
                            "--boot" => out.boot = Some(value.into()),
                            "--opsys" => out.opsys = Some(value.into()),
                            "--config" => out.config = Some(value.into()),
                            "--output-dir" => out.output = value.into(),
                            "--max-insns" => out.max_insns = count(&arg, &value, false)?,
                            "--warmup-insns" => out.warmup_insns = count(&arg, &value, true)?,
                            _ => {
                                target_set = true;
                                out.target = match value.as_str() {
                                    "menus" => HarnessTarget::Menus,
                                    "recovery" => HarnessTarget::Recovery,
                                    "ecm" => HarnessTarget::Ecm,
                                    "ecm-link" => HarnessTarget::EcmLink,
                                    "ecm-link-1367" => HarnessTarget::EcmLink1367,
                                    "security-link-1367" => HarnessTarget::SecurityLink1367,
                                    "dtc-link-1367" => HarnessTarget::DtcLink1367,
                                    "native-manual" => HarnessTarget::NativeManual,
                                    "engine-data-1367" => HarnessTarget::EngineData1367,
                                    _ => return Err("--harness-target expects menus, recovery, ecm, ecm-link, ecm-link-1367, security-link-1367 or dtc-link-1367 or engine-data-1367".into()),
                                };
                            }
                        }
                        continue;
                    }
                    _ if arg.starts_with('-') => return Err(format!("unknown option {arg:?}; see --help")),
                    _ if arg.starts_with("MAX_INSNS=") || arg.starts_with("WARMUP_INSNS=") => return Err(format!("{arg:?} is an environment assignment; put it before the command or use --max-insns/--warmup-insns")),
                    _ => {}
                }
            }
            if out.card.replace(arg.into()).is_some() {
                return Err("only one CARD_IMAGE may be specified".into());
            }
        }
        if out.interactive_headless
            && (out.test_harness
                || out.replay.is_some()
                || out.candi_nano_ssh.is_some()
                || out.candi_nano_usb_token.is_some()
                || out.vcx_ssh.is_some()
                || out.ibus_dtc.is_some())
        {
            return Err("--interactive-headless is an offline manual session; cannot combine with automated navigation or live bridges".into());
        }
        if out.candi_firmware.is_some() {
            out.candi = true;
        }
        if let Some(target) = &out.vcx_ssh {
            tech2_emu::vcx::Connection {
                target: target.clone(),
                control_path: out.vcx_control.clone(),
                ecu_profile: out.vcx_ecu,
            }
            .validate()?;
        } else if out.vcx_control.is_some()
            || out.ibus_dtc.is_some()
            || out.ecm_information
            || out.vcx_ecu != tech2_emu::vcx::EcuProfile::default()
        {
            return Err("--vcx-ssh-control and --ecm-information require --vcx-ssh".into());
        }
        if out.mock_vehicle {
            return Err("mock vehicle transport is not implemented; the old hook targeted a buffer helper, not vehicle preconditions".into());
        }
        if out.replay.is_some() && out.test_harness {
            return Err("--replay-keys cannot be combined with --test-harness".into());
        }
        out.headless |= out.test_harness || out.replay.is_some();
        if out.ibus_dtc.is_some() {
            if out.ecm_information
                || out.card.is_some()
                || out.replay.is_some()
                || out.test_harness
                || out.candi
                || out.research
                || out.fast_boot
                || out.strict
                || out.vcx_ecu != tech2_emu::vcx::EcuProfile::default()
            {
                return Err("--ibus-dtc is a standalone host diagnostic read; cannot combine with guest or ECU-profile options".into());
            }
            out.headless = true;
        }
        if out.headless && out.vcx_ssh.is_some() && out.ibus_dtc.is_none() {
            return Err("--vcx-ssh enables the GUI host page; use the vcx-vin binary for a headless live read".into());
        }
        out.research |= out.test_harness || out.fast_boot;
        if out.candi_on_demand && (!cfg!(feature = "load-test") || !out.candi_native_link || !out.research) {
            return Err("--candi-on-demand requires the experimental load-test build, --research-harness and --candi-native-link".into());
        }

        if out.ecm_information && (out.headless || !out.research) {
            return Err("--ecm-information requires a GUI build and --research-harness; use vcx-vin for a headless live read".into());
        }
        if out.candi_native_link && !out.research {
            return Err("--candi-native-link requires research mode".into());
        }
        if out.candi_nano_clear_dtc
            && (out.candi_nano_usb_token.is_none() || out.target != HarnessTarget::DtcLink1367)
        {
            return Err("--candi-nano-clear-dtc requires direct Nano USB and dtc-link-1367".into());
        }
        if let Some(token) = &out.candi_nano_usb_token {
            if token.len() != 32
                || !token.bytes().all(|b| b.is_ascii_hexdigit())
                || !out.candi_native_link
                || !out.test_harness
                || !out.headless
                || out.candi_nano_ssh.is_some()
                || out.vcx_ssh.is_some()
                || out.replay.is_some()
                || !matches!(
                    out.target,
                    HarnessTarget::SecurityLink1367
                        | HarnessTarget::DtcLink1367
                        | HarnessTarget::EngineData1367
                        | HarnessTarget::EcmLink1367
                        | HarnessTarget::NativeManual
                )
            {
                return Err("Direct Nano USB requires a 32-hex app token, native link and a live native harness target; cannot combine bridges or manual/replay sessions".into());
            }
        }
        if out.candi_chipsoft_audible
            && (out.candi_chipsoft_usb_token.is_none()
                || out.target != HarnessTarget::NativeManual
                || out.candi_chipsoft_seeds
                || out.candi_chipsoft_symbol_only
                || out.candi_seatbelt_audible)
        {
            return Err(
                "Chipsoft audible mode requires direct USB native-manual and exclusive authority"
                    .into(),
            );
        }
        if out.candi_chipsoft_seeds
            && (out.candi_chipsoft_usb_token.is_none()
                || out.target != HarnessTarget::NativeManual
                || out.candi_chipsoft_symbol_only
                || out.candi_seatbelt_audible)
        {
            return Err(
                "Chipsoft seed collection requires direct USB native-manual without a write mode"
                    .into(),
            );
        }
        if out.candi_chipsoft_symbol_only
            && (out.candi_chipsoft_usb_token.is_none() || out.target != HarnessTarget::DtcLink1367)
        {
            return Err(
                "Symbol Only mode requires direct Chipsoft USB with the authorized 1367 target"
                    .into(),
            );
        }
        if let Some(token) = &out.candi_chipsoft_usb_token {
            if token.len() != 32
                || !token.bytes().all(|b| b.is_ascii_hexdigit())
                || !out.candi_native_link
                || !out.test_harness
                || !out.headless
                || !matches!(
                    out.target,
                    HarnessTarget::DtcLink1367 | HarnessTarget::NativeManual
                )
                || out.candi_nano_usb_token.is_some()
                || out.candi_nano_ssh.is_some()
                || out.vcx_ssh.is_some()
                || out.replay.is_some()
                || out.ibus_dtc.is_some()
                || out.candi_nano_clear_dtc
                || out.candi_seatbelt_audible
            {
                return Err("Chipsoft USB requires a valid app token and native dtc-link-1367 or native-manual read harness, without another bridge or write mode".into());
            }
        }
        if out.candi_seatbelt_audible
            && (out.candi_j2534_adapter != tech2_emu::vcx::J2534Adapter::ChipsoftPro
                || out.candi_nano_ssh.is_none()
                || !out.candi_native_link
                || !out.test_harness
                || out.target != HarnessTarget::DtcLink1367
                || out.replay.is_some())
        {
            return Err(
                "Seatbelt mode requires Chipsoft native SSH with dtc-link-1367 and no replay"
                    .into(),
            );
        }
        if out.candi_j2534_adapter == tech2_emu::vcx::J2534Adapter::ChipsoftPro
            && out.candi_nano_ssh.is_none()
        {
            return Err("Chipsoft adapter requires the native SSH bridge".into());
        }
        if let Some(target) = &out.candi_nano_ssh {
            if !out.candi_native_link || out.vcx_ssh.is_some() || out.ibus_dtc.is_some() {
                return Err("--candi-nano-ssh requires --candi-native-link and cannot use host diagnostic modes".into());
            }
            tech2_emu::vcx::Connection {
                target: target.clone(),
                control_path: out.candi_nano_control.clone(),
                ecu_profile: tech2_emu::vcx::EcuProfile::default(),
            }
            .validate()?;
        } else if out.candi_nano_control.is_some() {
            return Err("--candi-nano-control requires --candi-nano-ssh".into());
        }
        if out.strict && out.research {
            return Err("--strict cannot be combined with research mode".into());
        }
        if target_set && !out.test_harness {
            return Err("--harness-target requires --test-harness".into());
        }
        Ok(out)
    }
}

pub const HELP: &str = "Tech2 Hardware & Software Emulator
Usage: tech2-emu [OPTIONS] [CARD_IMAGE]
  --ibus-dtc bcm|cim      Read VIN-1367 I-bus DTCs via host bridge (requires --vcx-ssh); no guest boot
  --candi-nano-ssh HOST    Native raw CAN bridge through Windows J2534 (requires native link)
  --candi-chipsoft-seeds           Original firmware security collection; manual vehicle selection
  --candi-chipsoft-audible         Scoped BCM audible operation for current 2004 vehicle
  --candi-chipsoft-symbol-only      Scoped BCM reversal with fresh per-run authority
  --candi-chipsoft-usb-token TOKEN  Direct Android Chipsoft USB; original firmware read-only
  --candi-seatbelt-audible Explicit constrained BCM seatbelt operation; requires per-run authority
  --candi-j2534-adapter nano|chipsoft-pro Installed Windows driver profile; default nano
  --candi-nano-usb-token TOKEN Android app loopback transport for native live harness
  --candi-nano-clear-dtc   Explicitly permit original firmware DTC clearing in USB dtc-link-1367 sessions
  --candi-nano-control PATH SSH multiplex control socket for native bridge
  --headless, -h           Run without a window; verify guest splash
  --candi-on-demand       Experimental: initialize native CANdi at first guest dependency
  --interactive-headless  Manual LCD/key mailbox session, offline, 30-minute deadline
  --verbose, -v           Write navigation/command observations to trace.jsonl
  --trace-calls           Also log every stepped JSR/BSR and RTS/RTD/RTR (large)
  --replay-keys PATH      Replay instruction-count/encoder-code rows headlessly
  --test-harness          Run the research menu smoke test (uses patches/HLE)
  --harness-target TARGET menus, recovery, ecm, ecm-link, ecm-link-1367, security-link-1367 (2008 Get Security Access), dtc-link-1367 (native DTC menus), or engine-data-1367 (native engine data)
  --research-harness      Enable legacy ROM/HLE experiments
  --vcx-ssh USER@HOST      Enable host ECM Information via Windows VCX helper
  --vcx-ecu PROFILE        Identification profile: t8, me96-1367 or ibus-bcm-1367
  --vcx-ssh-control PATH   Reuse an authenticated SSH control socket
  --ecm-information       Navigate 2004/9-3 diagnostics and open host live VIN page
  --strict                Require unmodified native execution
  --candi                 Run local CANDi ROM on a bounded research CPU worker
  --candi-native-link     Research UART link + CANdi console; no adapter
  --candi-firmware PATH   MSI CANDi ROM to execute (implies --candi; no vehicle TX)
  --fast-boot              Research-only: bypass POST failures
  --mock-vehicle           Unsupported: returns an explicit input error
  --boot PATH              Boot EPROM (default: eprom.bin)
  --opsys PATH             Download image (needed for research only)
  --config PATH            Tech2Win configuration
  --max-insns N            Positive instruction budget (default: 80000000)
  --warmup-insns N         GUI warmup budget (default: 30000000)
  --output-dir PATH        Captures, tech2.log, and report.json (default: .)
  --help                  Show this help
Exit status: 0 verified success, 1 guest failure, 2 input error,
             3 incomplete/budget/deadline, 4 output or GUI failure.
HEADLESS, TEST_HARNESS, MAX_INSNS and WARMUP_INSNS are also supported.";

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn parse(a: &[&str]) -> Result<Options, String> {
        Options::parse(a.iter().map(|s| s.to_string()), |_| None)
    }
    #[test]
    fn android_audible_is_explicit_and_mutually_exclusive() {
        let args = [
            "--test-harness",
            "--harness-target",
            "native-manual",
            "--candi-native-link",
            "--candi-chipsoft-usb-token",
            "0123456789abcdef0123456789abcdef",
            "--candi-chipsoft-audible",
        ];
        assert!(parse(&args).unwrap().candi_chipsoft_audible);
        assert!(parse(&["--candi-chipsoft-audible"]).is_err());
        for flag in [
            "--candi-chipsoft-seeds",
            "--candi-chipsoft-symbol-only",
            "--candi-seatbelt-audible",
            "--candi-nano-clear-dtc",
        ] {
            let mut a = args.to_vec();
            a.push(flag);
            assert!(parse(&a).is_err());
        }
        let mut wrong = args;
        wrong[2] = "dtc-link-1367";
        assert!(parse(&wrong).is_err());
    }

    #[test]
    fn chipsoft_seed_mode_is_explicit_manual_and_cannot_write() {
        let args = [
            "--test-harness",
            "--harness-target",
            "native-manual",
            "--candi-native-link",
            "--candi-chipsoft-usb-token",
            "0123456789abcdef0123456789abcdef",
            "--candi-chipsoft-seeds",
        ];
        assert!(parse(&args).unwrap().candi_chipsoft_seeds);
        assert!(parse(&["--candi-chipsoft-seeds"]).is_err());
        for extra in [
            "--candi-chipsoft-symbol-only",
            "--candi-seatbelt-audible",
            "--candi-nano-clear-dtc",
        ] {
            let mut a = args.to_vec();
            a.push(extra);
            assert!(parse(&a).is_err());
        }
    }

    #[test]
    fn native_manual_card_initialization_does_not_enable_seed_permissions() {
        let options = parse(&["--test-harness", "--harness-target", "native-manual",
            "--candi-native-link", "--candi-chipsoft-usb-token", "0123456789abcdef0123456789abcdef"]).unwrap();
        assert!(options.uses_ssa_flash());
        assert!(!options.candi_chipsoft_seeds);
        assert!(!options.candi_chipsoft_audible);
        assert!(!options.candi_chipsoft_symbol_only);
        assert!(!parse(&[]).unwrap().uses_ssa_flash());
        assert!(!parse(&["--test-harness", "--harness-target", "native-manual"]).unwrap().uses_ssa_flash());
    }

    #[test]
    fn chipsoft_requires_native_ssh_and_accepts_explicit_seed_target() {
        assert!(parse(&["--candi-j2534-adapter", "chipsoft-pro"]).is_err());
        assert!(parse(&["--candi-j2534-adapter", "wrong"]).is_err());
        let mut a = vec![
            "--test-harness",
            "--harness-target",
            "ecm-link-1367",
            "--candi-native-link",
            "--candi-nano-ssh",
            "user@host",
            "--candi-j2534-adapter",
            "chipsoft-pro",
        ];
        assert_eq!(
            parse(&a).unwrap().candi_j2534_adapter,
            tech2_emu::vcx::J2534Adapter::ChipsoftPro
        );
        a[2] = "security-link-1367";
        assert_eq!(parse(&a).unwrap().target, HarnessTarget::SecurityLink1367);
    }
    #[test]
    fn seatbelt_mode_requires_chipsoft_native_manual_target() {
        assert!(parse(&["--candi-seatbelt-audible"]).is_err());
        let mut a = vec![
            "--test-harness",
            "--harness-target",
            "dtc-link-1367",
            "--candi-native-link",
            "--candi-nano-ssh",
            "user@host",
            "--candi-j2534-adapter",
            "chipsoft-pro",
            "--candi-seatbelt-audible",
        ];
        assert!(parse(&a).unwrap().candi_seatbelt_audible);
        a[7] = "nano";
        assert!(parse(&a).is_err());
        a[7] = "chipsoft-pro";
        a[2] = "security-link-1367";
        assert!(parse(&a).is_err());
    }
    #[test]
    fn chipsoft_usb_rejects_mixed_transports_and_write_modes() {
        let base = [
            "--test-harness",
            "--harness-target",
            "dtc-link-1367",
            "--candi-native-link",
            "--candi-chipsoft-usb-token",
            "0123456789abcdef0123456789abcdef",
        ];
        assert!(parse(&base).unwrap().candi_chipsoft_usb_token.is_some());
        for extra in [
            &["--candi-nano-usb-token", "0123456789abcdef0123456789abcdef"][..], // gitleaks:allow -- synthetic argument-validation token
            &["--candi-nano-ssh", "user@host"],
            &["--candi-seatbelt-audible"],
            &["--candi-nano-clear-dtc"],
        ] {
            let mut a = base.to_vec();
            a.extend(extra);
            assert!(parse(&a).is_err());
        }
        let mut a = base;
        a[5] = "invalid";
        assert!(parse(&a).is_err());
        a = base;
        a[2] = "security-link-1367";
        assert!(parse(&a).is_err());
    }
    #[test]
    fn manual_chipsoft_session_cannot_reuse_previous_vehicle_write_authority() {
        let base = [
            "--test-harness",
            "--harness-target",
            "native-manual",
            "--candi-native-link",
            "--candi-chipsoft-usb-token",
            "0123456789abcdef0123456789abcdef",
        ];
        assert_eq!(parse(&base).unwrap().target, HarnessTarget::NativeManual);
        for flag in [
            "--candi-chipsoft-symbol-only",
            "--candi-seatbelt-audible",
            "--candi-nano-clear-dtc",
        ] {
            let mut a = base.to_vec();
            a.push(flag);
            assert!(parse(&a).is_err());
        }
    }
    #[test]
    fn clear_mode_requires_explicit_usb_dtc_session() {
        assert!(parse(&["--candi-nano-clear-dtc"]).is_err());
        let base = [
            "--test-harness",
            "--harness-target",
            "dtc-link-1367",
            "--candi-native-link",
            "--candi-nano-usb-token",
            "0123456789abcdef0123456789abcdef",
        ];
        assert!(!parse(&base).unwrap().candi_nano_clear_dtc);
        let mut args = base.to_vec();
        args.push("--candi-nano-clear-dtc");
        assert!(parse(&args).unwrap().candi_nano_clear_dtc);
        args[2] = "security-link-1367";
        assert!(parse(&args).is_err());
        args[2] = "ecm-link-1367";
        assert!(parse(&args).is_err());
        args[2] = "engine-data-1367";
        assert!(parse(&args).is_err());
        args.pop();
        assert_eq!(parse(&args).unwrap().target, HarnessTarget::EngineData1367);
    }

    #[test]
    fn direct_usb_requires_native_harness_and_rejects_competing_bridges() {
        let base = [
            "--test-harness",
            "--harness-target",
            "security-link-1367",
            "--candi-native-link",
            "--candi-nano-usb-token",
            "0123456789abcdef0123456789abcdef",
        ];
        assert!(parse(&base).is_ok());
        for extra in ["--interactive-headless", "--candi-nano-ssh"] {
            let mut a = base.to_vec();
            a.push(extra);
            if extra.ends_with("ssh") {
                a.push("user@host");
            }
            assert!(parse(&a).is_err());
        }
        let mut a = base.to_vec();
        a[2] = "menus";
        assert!(parse(&a).is_err());
        a[2] = "security-link-1367";
        a[5] = "bad";
        assert!(parse(&a).is_err());
    }
    #[test]
    fn manual_frontend_is_offline_and_excludes_automatic_navigation() {
        let opts = parse(&[
            "--interactive-headless",
            "--research-harness",
            "--candi-native-link",
        ])
        .unwrap();
        assert!(opts.headless && opts.interactive_headless && !opts.test_harness);
        for extra in [
            vec!["--test-harness"],
            vec!["--replay-keys", "keys.txt"],
            vec!["--candi-nano-ssh", "user@host"],
            vec!["--vcx-ssh", "user@host"],
        ] {
            let mut args = vec!["--interactive-headless"];
            args.extend(extra);
            assert!(parse(&args).is_err());
        }
    }
    #[test]
    fn invalid_arguments_are_errors() {
        for a in [
            vec!["--typo"],
            vec!["--max-insns", "0"],
            vec!["--max-insns", "abc"],
            vec!["a", "b"],
            vec!["MAX_INSNS=1"],
            vec!["--boot"],
            vec!["--strict", "--test-harness"],
        ] {
            assert!(parse(&a).is_err(), "{a:?}");
        }
    }
    #[test]
    fn candi_flag_is_opt_in() {
        assert!(parse(&["--candi-native-link"]).is_err());
        assert!(
            parse(&["--research-harness", "--candi-native-link"])
                .unwrap()
                .candi_native_link
        );
        assert!(parse(&["--headless", "--candi-native-link"]).is_err());
        assert!(
            parse(&["--test-harness", "--candi-native-link"])
                .unwrap()
                .candi_native_link
        );
        let o = parse(&["--candi"]).unwrap();
        assert!(o.candi);
        let o = parse(&["--candi-firmware", "/tmp/candi.bin"]).unwrap();
        assert!(o.candi);
        assert_eq!(
            o.candi_firmware.as_deref(),
            Some(Path::new("/tmp/candi.bin"))
        );
        assert!(!parse(&[]).unwrap().candi);
    }

    #[test]
    fn live_vcx_flags_do_not_silently_enable_an_unwired_headless_transport() {
        assert!(parse(&["--headless", "--vcx-ssh", "user@host"])
            .unwrap_err()
            .contains("vcx-vin"));
        assert!(parse(&["--vcx-ssh-control", "/tmp/socket"]).is_err());
        assert!(parse(&["--ecm-information"]).is_err());
        assert!(parse(&["--vcx-ssh", "user@host;command"]).is_err());
        if cfg!(feature = "gui") {
            let opts = parse(&[
                "--research-harness",
                "--vcx-ssh",
                "user@host",
                "--ecm-information",
            ])
            .unwrap();
            assert!(opts.ecm_information);
        }
    }

    #[test]
    fn native_bridge_requires_native_link_and_excludes_host_reads() {
        assert!(parse(&["--candi-nano-ssh", "user@host"]).is_err());
        assert!(parse(&["--candi-nano-control", "/tmp/control"]).is_err());
        assert!(parse(&[
            "--research-harness",
            "--candi-native-link",
            "--candi-nano-ssh",
            "user@host",
            "--vcx-ssh",
            "user@host"
        ])
        .is_err());
        let options = parse(&[
            "--test-harness",
            "--harness-target",
            "ecm-link-1367",
            "--candi-native-link",
            "--candi-nano-ssh",
            "user@host",
        ])
        .unwrap();
        assert!(options.headless && options.candi_native_link);
        assert_eq!(options.target, HarnessTarget::EcmLink1367);
    }
    #[test]
    fn ibus_dtc_is_an_explicit_headless_host_operation() {
        let opts = parse(&["--ibus-dtc", "cim", "--vcx-ssh", "user@host"]).unwrap();
        assert!(opts.headless);
        assert_eq!(opts.ibus_dtc, Some(tech2_emu::ibus_dtc::Module::Cim));
        for args in [
            vec!["--ibus-dtc", "bcm"],
            vec!["--ibus-dtc", "all", "--vcx-ssh", "user@host"],
            vec!["--ibus-dtc", "bcm", "--vcx-ssh", "user@host", "card.bin"],
            vec![
                "--ibus-dtc",
                "bcm",
                "--vcx-ssh",
                "user@host",
                "--research-harness",
            ],
        ] {
            assert!(parse(&args).is_err());
        }
    }

    #[test]
    fn unavailable_mock_transport_is_explicitly_rejected() {
        assert!(parse(&["--mock-vehicle"])
            .unwrap_err()
            .contains("not implemented"));
    }

    #[test]
    fn false_environment_flags_stay_false() {
        let o = Options::parse(Vec::new(), |k| {
            matches!(k, "HEADLESS" | "TECH2_RESEARCH_HARNESS").then(|| "0".into())
        })
        .unwrap();
        assert_eq!(o.headless, !cfg!(feature = "gui"));
        assert!(!o.research);
    }
    #[test]
    fn explicit_budget_and_dash_filename() {
        let o = parse(&[
            "--test-harness",
            "--max-insns",
            "42000000",
            "--",
            "-card.bin",
        ])
        .unwrap();
        assert!(o.headless && o.research);
        assert_eq!(o.max_insns, 42_000_000);
        assert_eq!(o.card, Some(PathBuf::from("-card.bin")));
    }
}
