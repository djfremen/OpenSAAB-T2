// SPDX-License-Identifier: MPL-2.0
//! Card-derived TIS2000 transfer prompt for every diagnostic-card language.
//!
//! The original firmware shows the transfer instructions from its own message
//! database. Each localized Tech 2 card stores that message as NUL-terminated
//! lines: a record line carrying a 3-byte index prefix and the title, a dashed
//! or blank separator, then numbered steps. The title always names `TIS2000`
//! and the step list always names the `RS232` cable, so the block is located by those
//! untranslated anchors and matched with the card's own bytes. No translated
//! literals are compiled in.
//!
//! A screen matches when the title, separator, step 1 and RS232 step appear
//! on successive rows, as the firmware prints the message line by line.
//!
//! The LCD text capture (`screen.txt`) keeps printable ASCII and blanks other
//! codes, and a card may map accented or Cyrillic characters to its own glyph
//! codes. Non-ASCII card bytes therefore match any single screen cell, while
//! every ASCII byte must match exactly.
use serde_json::{json, Value};
use std::{fs, path::Path, time::UNIX_EPOCH};

const ANCHOR: &[u8] = b"TIS2000";
const CABLE: &[u8] = b"RS232";
const MAX_LINE: usize = 64;
const MAX_TITLE: usize = 3;
const MAX_PROMPTS: usize = 4;
const CACHE: &str = "security-prompt.json";
const CACHE_SCHEMA: u64 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferPrompt {
    /// Title line(s), record prefix removed and outer spaces trimmed.
    pub title: Vec<Vec<u8>>,
    /// Separator below the title; empty for the Dutch card's blank row.
    pub separator: Vec<u8>,
    /// Step 1 line, outer spaces trimmed.
    pub step: Vec<u8>,
    /// The line naming the RS232 cable, outer spaces trimmed.
    pub cable: Vec<u8>,
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + needle.len() <= hay.len() {
        let p = hay[i..=hay.len() - needle.len()]
            .iter()
            .position(|b| *b == needle[0])?;
        i += p;
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn trim(mut b: &[u8]) -> &[u8] {
    while let [b' ', rest @ ..] = b {
        b = rest;
    }
    while let [rest @ .., b' '] = b {
        b = rest;
    }
    b
}

/// Bounded NUL-terminated line containing `pos`, as (start, end) with end at
/// the NUL. Message lines are short; anything longer is not a message line,
/// which also keeps extraction linear on cards without NUL separators.
fn line_at(card: &[u8], pos: usize) -> Option<(usize, usize)> {
    let reach = MAX_LINE + 4;
    let low = pos.saturating_sub(reach);
    let start = match card[low..pos].iter().rposition(|b| *b == 0) {
        Some(p) => low + p + 1,
        None if low == 0 => 0,
        None => return None,
    };
    let high = (pos + reach).min(card.len());
    let end = match card[pos..high].iter().position(|b| *b == 0) {
        Some(p) => pos + p,
        None if high == card.len() => card.len(),
        None => return None,
    };
    Some((start, end))
}

fn next_line(card: &[u8], end: usize) -> Option<(usize, usize)> {
    if end < card.len() {
        line_at(card, end + 1)
    } else {
        None
    }
}

fn prev_line(card: &[u8], start: usize) -> Option<(usize, usize)> {
    if start >= 1 {
        line_at(card, start - 1)
    } else {
        None
    }
}

fn dashes(line: &[u8]) -> bool {
    line.len() >= 16 && line.iter().all(|b| *b == b'-')
}

fn text_line(line: &[u8]) -> bool {
    line.len() <= MAX_LINE && line.iter().all(|b| *b >= 0x20 && *b != 0x7f)
}

fn starts_record(line: &[u8]) -> bool {
    line.first().is_some_and(|b| (1..0x20).contains(b))
}

fn block(card: &[u8], hit: usize) -> Option<TransferPrompt> {
    // Title: the record-prefixed line containing or preceding the anchor, plus
    // continuation lines up to the dashed or blank separator; at most
    // MAX_TITLE lines.
    let (s, e) = line_at(card, hit)?;
    let mut title = vec![(s, e)];
    while !starts_record(&card[title[0].0..title[0].1]) {
        if title.len() >= MAX_TITLE {
            return None;
        }
        let prev = prev_line(card, title[0].0)?;
        if prev.0 == prev.1 {
            return None;
        }
        title.insert(0, prev);
    }
    let mut cursor = e;
    let mut separator = None;
    while separator.is_none() {
        let (ns, ne) = next_line(card, cursor)?;
        cursor = ne;
        if ns == ne || dashes(&card[ns..ne]) {
            separator = Some(card[ns..ne].to_vec());
        } else if title.len() >= MAX_TITLE {
            return None;
        } else {
            title.push((ns, ne));
        }
    }
    let mut lines = vec![];
    for (i, (ls, le)) in title.iter().enumerate() {
        let raw = &card[*ls..*le];
        let raw = if i == 0 { raw.get(3..).unwrap_or(&[]) } else { raw };
        if !text_line(raw) {
            return None;
        }
        let t = trim(raw);
        if !t.is_empty() {
            lines.push(t.to_vec());
        }
    }
    if lines.is_empty() || !lines.iter().any(|l| find(l, ANCHOR, 0).is_some()) {
        return None;
    }
    // Steps: "1." immediately after the separator; the cable step follows.
    let (ss, se) = next_line(card, cursor)?;
    let step = trim(&card[ss..se]);
    if !text_line(&card[ss..se]) || !step.starts_with(b"1.") || step.len() < 4 {
        return None;
    }
    let mut c = se;
    for _ in 0..6 {
        let (ns, ne) = next_line(card, c)?;
        c = ne;
        let line = &card[ns..ne];
        if find(line, CABLE, 0).is_some() {
            return text_line(line).then(|| TransferPrompt {
                title: lines,
                separator: separator.unwrap_or_default(),
                step: step.to_vec(),
                cable: trim(line).to_vec(),
            });
        }
    }
    None
}

/// All distinct transfer prompt blocks in a diagnostic card.
pub fn extract(card: &[u8]) -> Vec<TransferPrompt> {
    let mut out: Vec<TransferPrompt> = vec![];
    let mut at = 0;
    while let Some(hit) = find(card, ANCHOR, at) {
        at = hit + 1;
        if let Some(p) = block(card, hit) {
            if !out.contains(&p) {
                out.push(p);
                if out.len() >= MAX_PROMPTS {
                    break;
                }
            }
        }
    }
    out
}

/// ASCII must match exactly; card bytes outside printable ASCII match any cell.
fn row_contains(row: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && needle.len() <= row.len()
        && row.windows(needle.len()).any(|w| {
            w.iter()
                .zip(needle)
                .all(|(s, n)| !(0x20..0x7f).contains(n) || s == n)
        })
}

impl TransferPrompt {
    fn needles(&self) -> impl Iterator<Item = &Vec<u8>> {
        self.title
            .iter()
            .chain([&self.separator, &self.step, &self.cable])
    }

    /// Title line(s), separator, step 1 and the RS232 step, each on a later
    /// screen row than the last, as the original firmware lays out the message.
    pub fn matches(&self, screen: &[u8]) -> bool {
        let rows: Vec<&[u8]> = screen.split(|b| *b == b'\n').collect();
        let mut next = 0;
        for (index, needle) in self.needles().enumerate() {
            let blank_separator = index == self.title.len() && self.separator.is_empty();
            match rows[next.min(rows.len())..]
                .iter()
                .position(|r| {
                    if blank_separator {
                        trim(r).is_empty()
                    } else {
                        row_contains(r, needle)
                    }
                })
            {
                Some(p) => next += p + 1,
                None => return false,
            }
        }
        true
    }

    fn to_json(&self) -> Value {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        let shown = |b: &[u8]| {
            b.iter()
                .map(|x| if (0x20..0x7f).contains(x) { *x as char } else { '?' })
                .collect::<String>()
        };
        json!({"title":self.title.iter().map(|l| hex(l)).collect::<Vec<_>>(),
            "separator":hex(&self.separator),"step":hex(&self.step),"cable":hex(&self.cable),
            "preview":self.needles().map(|l| shown(l)).collect::<Vec<_>>()})
    }

    /// Cached prompts obey the same shape as extracted ones, or are ignored.
    fn from_json(v: &Value) -> Option<Self> {
        let unhex = |v: &Value, allow_empty: bool| -> Option<Vec<u8>> {
            let s = v.as_str()?;
            if (!allow_empty && s.is_empty()) || s.len() % 2 != 0 || s.len() > MAX_LINE * 2 {
                return None;
            }
            let bytes = (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
                .collect::<Option<Vec<u8>>>()?;
            text_line(&bytes).then_some(bytes)
        };
        let title = v["title"]
            .as_array()?
            .iter()
            .map(|v| unhex(v, false))
            .collect::<Option<Vec<_>>>()?;
        let p = Self {
            separator: unhex(&v["separator"], true)?,
            step: unhex(&v["step"], false)?,
            cable: unhex(&v["cable"], false)?,
            title,
        };
        (!p.title.is_empty()
            && p.title.len() <= MAX_TITLE
            && p.title.iter().any(|l| find(l, ANCHOR, 0).is_some())
            && (p.separator.is_empty() || dashes(&p.separator))
            && p.step.len() >= 4
            && p.step.starts_with(b"1.")
            && find(&p.cable, CABLE, 0).is_some())
        .then_some(p)
    }
}

fn card_key(card: &Path) -> Option<String> {
    let meta = fs::symlink_metadata(card).ok()?;
    if !meta.file_type().is_file() || meta.len() > super::CARD_SIZE as u64 {
        return None;
    }
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(format!("{}:{}", meta.len(), modified.as_nanos()))
}

/// How a lookup may use `security-prompt.json` beside a session's card.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cache {
    /// Processing gates derive the prompt from the card itself.
    Ignore,
    /// Display status reads a matching cache but never writes.
    Read,
    /// The live workflow poll stores the derived prompt for its own session.
    ReadWrite,
}

// A session directory directly inside the app's `runs` folder (which may
// itself be a link to another volume); never an evidence or other folder.
fn live_session(run: &Path) -> bool {
    run.parent().and_then(Path::file_name) == Some(std::ffi::OsStr::new("runs"))
        && fs::symlink_metadata(run).is_ok_and(|m| m.is_dir())
}

/// Prompts for a session's card, keyed to the card's size and modification time.
pub fn for_run(run: &Path, cache: Cache) -> Vec<TransferPrompt> {
    let card_path = run.join("card.bin");
    let Some(key) = card_key(&card_path) else {
        return vec![];
    };
    let cache_path = run.join(CACHE);
    if cache != Cache::Ignore {
        if let Ok(v) = super::read(&cache_path, 64 * 1024)
            .and_then(|b| serde_json::from_slice::<Value>(&b).map_err(|_| String::new()))
        {
            if v["schema"] == CACHE_SCHEMA && v["card"] == key.as_str() {
                if let Some(list) = v["prompts"].as_array() {
                    let parsed: Option<Vec<_>> = list.iter().map(TransferPrompt::from_json).collect();
                    if let Some(parsed) = parsed {
                        return parsed;
                    }
                }
            }
        }
    }
    let Ok(card) = super::read(&card_path, super::CARD_SIZE) else {
        return vec![];
    };
    let prompts = extract(&card);
    if cache == Cache::ReadWrite
        && live_session(run)
        && card_key(&card_path).as_deref() == Some(key.as_str())
    {
        let doc = json!({"schema":CACHE_SCHEMA,"card":key,
            "prompts":prompts.iter().map(TransferPrompt::to_json).collect::<Vec<_>>()});
        let temp = run.join(format!(".{CACHE}.{}", super::unique()));
        if super::write_json(&temp, &doc).is_ok() {
            if fs::rename(&temp, &cache_path).is_err() {
                let _ = fs::remove_file(&temp);
            }
        }
    }
    prompts
}

/// Whether a screen can show a card prompt at all (cheap pre-check).
pub fn mentions_anchor(screen: &[u8]) -> bool {
    find(screen, ANCHOR, 0).is_some()
}

/// Card-prompt match of an LCD text capture.
pub fn matches_any(prompts: &[TransferPrompt], screen: &[u8]) -> bool {
    prompts.iter().any(|p| p.matches(screen))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// Message-database bytes in the layout of the 148.000 cards: index records,
    /// a record-prefixed title, the dashed separator, then numbered steps.
    pub fn message(prefix: [u8; 3], title: &[&[u8]], steps: &[&[u8]]) -> Vec<u8> {
        let mut b = vec![0u8];
        for record in [[prefix[0], prefix[1], 0x74], [prefix[0], prefix[1], 0x98]] {
            b.extend_from_slice(&record);
            b.push(0);
        }
        b.extend_from_slice(&prefix);
        for (i, line) in title.iter().enumerate() {
            if i > 0 {
                b.push(0);
            }
            b.extend_from_slice(line);
        }
        b.push(0);
        b.extend_from_slice(&[b'-'; 39]);
        b.push(0);
        for step in steps {
            b.extend_from_slice(step);
            b.push(0);
        }
        b.extend_from_slice(&[0xff, 0, 0, 0]);
        b
    }
    // Exact bytes of the German 148.000 card block (Latin-1 umlauts).
    pub const DE_TITLE: &[u8] = b"Sicherheitszugang v. TIS2000 erforderl.";
    pub const DE_STEPS: [&[u8]; 4] = [
        b"1. Tech2 von Fahrzeug abbauen.",
        b"2. Tech2 \xfcber RS232-Kabel an TIS2000-PC",
        b"anschlie\xdfen.",
        b"6. Zu Aufgabe zur\xfcckkehren.",
    ];
    // Synthetic non-Latin block: high-byte words around the untranslated anchors.
    pub const CYR_TITLE: &[u8] = b"\xd2\xf0\xe5\xe1\xf3\xe5\xf2\xf1\xff \xe4\xee\xf1\xf2\xf3\xef TIS2000";
    pub const CYR_STEPS: [&[u8]; 2] = [
        b"1. \xce\xf2\xea\xeb\xfe\xf7\xe8\xf2\xe5 Tech 2 \xee\xf2 \xe0\xe2\xf2\xee.",
        b"2. \xcf\xee\xe4\xea\xeb\xfe\xf7\xe8\xf2\xe5 TIS2000 RS232.",
    ];

    pub fn card_with(blocks: &[Vec<u8>]) -> Vec<u8> {
        let mut card = vec![0x42u8; 4096];
        for b in blocks {
            card.extend_from_slice(b);
            card.extend_from_slice(&[0x42; 512]);
        }
        card
    }
    /// LCD text capture as the emulator writes it: 40 columns, non-ASCII blank.
    pub fn screen(lines: &[&[u8]], map: impl Fn(u8) -> u8) -> Vec<u8> {
        let mut out = vec![];
        for row in 0..15 {
            let mut cells = [b' '; 40];
            if let Some(line) = lines.get(row) {
                for (c, b) in line.iter().take(40).enumerate() {
                    cells[c] = map(*b);
                }
            }
            out.extend(cells.iter().map(|b| if (0x20..0x7f).contains(b) { *b } else { b' ' }));
            out.push(b'\n');
        }
        out
    }
    pub fn same(b: u8) -> u8 {
        b
    }

    #[test]
    fn extracts_exact_localized_block_and_ignores_other_tis_messages() {
        let task = message(
            [2, 0x55, 0xdf],
            &[b"F\xfcr diese Aufgabe ist der", b"Sicherheitszugang von TIS2000", b"erforderlich."],
            &[b"", b"Alle Fahrzeugschl\xfcssel m\xfcssen in einem"],
        );
        let flash = b"\0\0ECU mit Hilfe von TIS2000 programmieren.\0\0".to_vec();
        let de = message([2, 0x53, 0xe3], &[DE_TITLE], &DE_STEPS);
        let found = extract(&card_with(&[flash, task, de.clone(), de]));
        assert_eq!(
            found,
            vec![TransferPrompt {
                title: vec![DE_TITLE.to_vec()],
                separator: vec![b'-'; 39],
                step: DE_STEPS[0].to_vec(),
                cable: DE_STEPS[1].to_vec(),
            }]
        );
        let en = message(
            [2, 0x2a, 0xc2],
            &[b" You need Security Access from TIS2000"],
            &[b"1. Disconnect Tech 2 from Vehicle.", b"2. Connect Tech 2 to TIS2000 PC via", b"   RS232 cable."],
        );
        let found = extract(&card_with(&[en]));
        assert_eq!(found[0].title, vec![b"You need Security Access from TIS2000".to_vec()]);
        assert_eq!(found[0].step, b"1. Disconnect Tech 2 from Vehicle.".to_vec());
        // A prompt without its RS232 transfer step is not the TIS transfer prompt.
        let partial = message([2, 0x53, 0xe3], &[DE_TITLE], &DE_STEPS[..1]);
        assert!(extract(&card_with(&[partial])).is_empty());
        assert!(extract(&[0x42; 8192]).is_empty());
    }

    #[test]
    fn localized_screen_matches_whatever_glyph_codes_the_lcd_uses() {
        let prompts = extract(&card_with(&[
            message([2, 0x53, 0xe3], &[DE_TITLE], &DE_STEPS),
            message([3, 0x11, 0x40], &[CYR_TITLE], &CYR_STEPS),
        ]));
        assert_eq!(prompts.len(), 2);
        let dashes = [b'-'; 39];
        for (title, steps) in [(DE_TITLE, &DE_STEPS[..]), (CYR_TITLE, &CYR_STEPS[..])] {
            let lines = [title, &dashes[..], steps[0], steps[1]];
            // Card bytes, remapped glyph codes, and 7-bit glyph codes.
            for map in [same as fn(u8) -> u8, |b| if b >= 0x80 { b ^ 0x40 } else { b }, |b| if b >= 0x80 { b & 0x7f } else { b }] {
                assert!(matches_any(&prompts, &screen(&lines, map)));
            }
            // Title alone, a missing separator or cable step, steps above the
            // title, or a changed ASCII anchor do not match.
            assert!(!matches_any(&prompts, &screen(&lines[..2], same)));
            assert!(!matches_any(&prompts, &screen(&lines[..3], same)));
            assert!(!matches_any(&prompts, &screen(&[title, steps[0], steps[1]], same)));
            assert!(!matches_any(&prompts, &screen(&[steps[0], title], same)));
            let wrong: Vec<u8> = title.iter().map(|b| if *b == b'0' { b'8' } else { *b }).collect();
            assert!(!matches_any(&prompts, &screen(&[&wrong, &dashes, steps[0]], same)));
        }
        // Prompts from one card do not accept another language's screen.
        let de_only = extract(&card_with(&[message([2, 0x53, 0xe3], &[DE_TITLE], &DE_STEPS)]));
        assert!(!matches_any(&de_only, &screen(&[CYR_TITLE, &dashes, CYR_STEPS[0]], same)));
    }

    #[test]
    fn dutch_transfer_requires_its_blank_separator_and_both_steps() {
        // Exact message shape from the catalog-verified Dutch 148.000 card.
        let card = card_with(&[b"\0\x02\x54\x1aU heeft een veiligheidstoegang van\0TIS2000 nodig\0\0\
            1. Tech2 van de auto ontkoppelen.\0\
            2. Tech2 aan de PC van TIS2000\0koppelen via kabel RS232.\0\xff\0".to_vec()]);
        let prompts = extract(&card);
        assert_eq!(prompts.len(), 1);
        let prompt = &prompts[0];
        assert!(prompt.separator.is_empty());
        assert_eq!(TransferPrompt::from_json(&prompt.to_json()), Some(prompt.clone()));
        let lines: [&[u8]; 6] = [
            b"U heeft een veiligheidstoegang van", b"TIS2000 nodig", b"",
            b"1. Tech2 van de auto ontkoppelen.", b"2. Tech2 aan de PC van TIS2000",
            b"koppelen via kabel RS232.",
        ];
        assert!(prompt.matches(&screen(&lines, same)));
        assert!(!prompt.matches(&screen(&lines[..5], same)));
        let no_blank: Vec<&[u8]> = lines.iter().enumerate()
            .filter_map(|(i, line)| (i != 2).then_some(*line)).collect();
        assert!(!prompt.matches(&screen(&no_blank, same)));
        let mut wrong = lines;
        wrong[3] = b"1. A different task.";
        assert!(!prompt.matches(&screen(&wrong, same)));
    }

    #[test]
    fn bounded_lines_keep_extraction_linear_and_titles_short() {
        // No NUL separators at all: every anchor is rejected in bounded time.
        let flood = b"TIS2000 ".repeat(32 * 1024);
        let started = std::time::Instant::now();
        assert!(extract(&flood).is_empty());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        // Longer titles than the cache accepts are never extracted.
        let four = message([2, 0x53, 0xe3], &[b"a", b"b", b"c", DE_TITLE], &DE_STEPS);
        assert!(extract(&card_with(&[four])).is_empty());
        let three = message([2, 0x53, 0xe3], &[b"a", b"b", DE_TITLE], &DE_STEPS);
        let found = extract(&card_with(&[three]));
        assert_eq!(found[0].title.len(), 3);
        assert_eq!(TransferPrompt::from_json(&found[0].to_json()), Some(found[0].clone()));
    }

    #[test]
    fn malformed_cache_is_ignored_without_panicking() {
        let p = &extract(&card_with(&[message([2, 0x53, 0xe3], &[DE_TITLE], &DE_STEPS)]))[0];
        let good = p.to_json();
        for (field, bad) in [("title", json!([""])), ("title", json!(["2d2d"])), ("step", json!("")),
            ("separator", json!("2d")), ("cable", json!("312e")), ("title", json!(["zz"]))] {
            let mut v = good.clone();
            v[field] = bad;
            assert!(TransferPrompt::from_json(&v).is_none(), "{field}");
        }
        assert!(!row_contains(b"anything", b""));
    }

    #[test]
    fn cache_round_trips_and_is_written_only_for_live_polls() {
        let root = std::env::temp_dir().join(format!("prompt-cache-{}", super::super::unique()));
        let run = root.join("runs/live");
        fs::create_dir_all(&run).unwrap();
        fs::write(run.join("card.bin"), card_with(&[message([2, 0x53, 0xe3], &[DE_TITLE], &DE_STEPS)])).unwrap();
        let expected = for_run(&run, Cache::Ignore);
        assert_eq!(expected.len(), 1);
        assert_eq!(for_run(&run, Cache::Read), expected);
        assert!(!run.join(CACHE).exists());
        assert_eq!(for_run(&run, Cache::ReadWrite), expected);
        assert!(run.join(CACHE).exists());
        assert_eq!(for_run(&run, Cache::Read), expected);
        // Preview.4 may have cached no prompt for an unchanged Dutch card.
        // Re-extract an old schema without letting a read-only lookup rewrite it.
        let legacy = json!({"schema":2,"card":card_key(&run.join("card.bin")).unwrap(),"prompts":[]});
        fs::write(run.join(CACHE), serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(for_run(&run, Cache::Read), expected);
        let unchanged: Value = serde_json::from_slice(&fs::read(run.join(CACHE)).unwrap()).unwrap();
        assert_eq!(unchanged, legacy);
        assert_eq!(for_run(&run, Cache::ReadWrite), expected);
        assert_eq!(for_run(&run, Cache::Read), expected);
        // A cache for a different card is ignored.
        fs::write(run.join("card.bin"), vec![0x42; 8192]).unwrap();
        assert!(for_run(&run, Cache::Read).is_empty());
        // Outside a sessions directory the poll never writes.
        let other = root.join("evidence");
        fs::create_dir_all(&other).unwrap();
        fs::copy(run.join("card.bin"), other.join("card.bin")).unwrap();
        for_run(&other, Cache::ReadWrite);
        assert!(!other.join(CACHE).exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// A real captured transfer screen against its card's own prompt:
    /// OPENSAAB_TRANSFER_SCREEN=<screen.txt> OPENSAAB_TRANSFER_CARD=<card.bin> cargo test -- --ignored real_transfer_screen
    #[test]
    #[ignore]
    fn real_transfer_screen() {
        let screen = fs::read(std::env::var("OPENSAAB_TRANSFER_SCREEN").expect("OPENSAAB_TRANSFER_SCREEN")).unwrap();
        let card = fs::read(std::env::var("OPENSAAB_TRANSFER_CARD").expect("OPENSAAB_TRANSFER_CARD")).unwrap();
        let prompts = extract(&card);
        assert_eq!(prompts.len(), 1);
        assert!(matches_any(&prompts, &screen));
        // The same capture minus its numbered steps is not the prompt.
        let cut: Vec<u8> = screen.split(|b| *b == b'\n').take(4).collect::<Vec<_>>().join(&b'\n');
        assert!(!matches_any(&prompts, &cut));
    }

    /// Integration check against real diagnostic cards (not distributed):
    /// OPENSAAB_LANGUAGE_CARDS=<dir of *.bin> cargo test --release -- --ignored real_cards
    #[test]
    #[ignore]
    fn real_cards() {
        let dir = std::env::var("OPENSAAB_LANGUAGE_CARDS").expect("OPENSAAB_LANGUAGE_CARDS");
        let mut names: Vec<_> = fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "bin")).collect();
        names.sort();
        assert!(!names.is_empty());
        for path in names {
            let card = fs::read(&path).unwrap();
            let started = std::time::Instant::now();
            let prompts = extract(&card);
            println!("{}: {} prompt(s) in {:?}", path.display(), prompts.len(), started.elapsed());
            for p in &prompts {
                println!("  {}", p.to_json());
            }
            assert_eq!(prompts.len(), 1, "{}", path.display());
            let p = &prompts[0];
            let mut lines: Vec<&[u8]> = p.title.iter().map(|l| l.as_slice()).collect();
            lines.extend([p.separator.as_slice(), &p.step, &p.cable]);
            assert!(p.matches(&screen(&lines, same)));
        }
    }
}
