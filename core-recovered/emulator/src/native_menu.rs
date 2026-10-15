// SPDX-License-Identifier: MPL-2.0
//! Read original LCD text and observed selection bars. No OCR or fabricated menu tree.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const KEYS: [(&str, u8); 24] = [
    ("S1", 10),
    ("S2", 6),
    ("S3", 7),
    ("S4", 8),
    ("EXIT", 1),
    ("ENTER", 16),
    ("UP", 9),
    ("DOWN", 12),
    ("LEFT", 11),
    ("RIGHT", 13),
    ("HELP", 25),
    ("YES", 14),
    ("NO", 20),
    ("F0", 24),
    ("F1", 4),
    ("F2", 19),
    ("F3", 23),
    ("F4", 3),
    ("F5", 18),
    ("F6", 22),
    ("F7", 2),
    ("F8", 17),
    ("F9", 21),
    ("SELECT", 16),
];
pub fn key_code(name: &str) -> Option<u8> {
    KEYS.iter()
        .find(|(key, _)| *key == name)
        .map(|(_, code)| *code)
}

pub fn inspect(screen: &str, selected_rows: &[usize], columns: usize, font_height: usize) -> Value {
    let lines: Vec<_> = screen.lines().collect();
    let list_kind = if screen.contains("Model Year") {
        "model_year"
    } else if screen.contains("Vehicle Type") {
        "vehicle_type"
    } else if screen.contains("Customer Functions") {
        "customer_functions"
    } else {
        ""
    };
    let list_label = |text: &str| -> Option<String> {
        let plain =
            if text.as_bytes().get(0) == Some(&b'(') && text.as_bytes().get(2) == Some(&b')') {
                text[3..].trim()
            } else {
                text
            };
        match list_kind {
            "model_year" if plain.len() == 4 && plain.bytes().all(|b| b.is_ascii_digit()) => {
                Some(plain.to_owned())
            }
            "vehicle_type" if text.starts_with("Saab ") => Some(text.to_owned()),
            "customer_functions"
                if [
                    "Engine Control",
                    "Transmission",
                    "Engine",
                    "Automatic Transmission",
                ]
                .contains(&text) =>
            {
                Some(text.to_owned())
            }
            _ => None,
        }
    };
    // Android's recognized list navigator reads the repeated selected item at
    // the bottom of the guest text grid. Observe that echo directly; no OCR.
    let footer = lines
        .iter()
        .enumerate()
        .rfind(|(_, line)| !line.trim().is_empty());
    let echo = footer.and_then(|(row, line)| {
        if list_kind.is_empty() || row + 3 < lines.len() {
            return None;
        }
        let matches: Vec<_> = lines[..row]
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                candidate.trim() == line.trim() && list_label(candidate.trim()).is_some()
            })
            .collect();
        (matches.len() == 1).then(|| (row, matches[0].0, line.trim()))
    });
    let mut entries = Vec::new();
    let mut rows = Vec::new();
    let mut selected = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let text = line.trim();
        let highlighted = selected_rows.contains(&row);
        rows.push(json!({"row":row,"text":text,"highlighted":highlighted}));
        if highlighted && !text.is_empty() {
            selected.push(text);
        }
        let bytes = text.as_bytes();
        if bytes.len() > 3 && bytes[0] == b'F' && bytes[1].is_ascii_digit() && bytes[2] == b':' {
            let key = &text[..2];
            entries.push(json!({"label":text[3..].trim(),"key":key,
                "encoder":key_code(key),"row":row,"selected":highlighted}));
        }
    }
    // Firmware footer labels occupy four fixed LCD quarters. Only report labels
    // actually present below y=210; key enablement is unknown in the guest.
    let mut soft_keys = Vec::new();
    let width = columns / 4;
    if width > 0 {
        for index in 0..4 {
            let mut parts = Vec::new();
            for (row, line) in lines.iter().enumerate() {
                if row * font_height < 210
                    || !list_kind.is_empty()
                    || screen.contains("Press [ENTER]") && screen.contains("Software Version")
                {
                    continue;
                }
                let part: String = line.chars().skip(index * width).take(width).collect();
                let part = part.trim();
                if !part.is_empty() {
                    parts.push(part.to_owned());
                }
            }
            let label = parts.join(" ");
            if !label.is_empty() {
                let key = format!("S{}", index + 1);
                soft_keys
                    .push(json!({"key":key,"encoder":key_code(&key),"label":label,"enabled":null}));
            }
        }
    }
    let selection = if let Some((_, row, text)) = echo {
        json!({"text":text,"row":row,"source":"guest_selected_item_footer"})
    } else if selected.len() == 1 {
        json!({"text":selected[0],"row":selected_rows.iter().find(|row|lines.get(**row).is_some_and(|line|!line.trim().is_empty())),"source":"guest_graphics_selection_bar"})
    } else {
        Value::Null
    };
    let list_entries:Vec<_> = lines.iter().enumerate().filter(|(row,_)|echo.is_none_or(|(footer,_,_)|*row!=footer))
        .filter_map(|(row,line)|list_label(line.trim()).map(|label|json!({"label":label,"text":line.trim(),"row":row,"selected":selection["row"]==row}))).collect();
    let selection_known = !selection.is_null();
    let mut value = json!({"schema":1,"source":"original_guest_lcd","title":lines.iter().find(|l| !l.trim().is_empty()).map(|l| l.trim()).unwrap_or(""),
        "text":screen,"rows":rows,"entries":entries,"selection":selection,"soft_keys":soft_keys,
        "list_kind":list_kind,"list_entries":list_entries,"selection_known":selection_known,"soft_key_enablement":"unknown",
        "columns":columns,"font_height":font_height});
    // A font/row-spacing redraw is not a different actionable menu. Preserve
    // selection, labels and order while excluding geometry from the revision.
    let signature = json!({"screen":screen_identity(&value),"selection":value["selection"]["text"],
        "entries":value["entries"].as_array().unwrap().iter().map(|e|json!([e["label"],e["key"]])).collect::<Vec<_>>(),
        "soft_keys":value["soft_keys"].as_array().unwrap().iter().map(|e|json!([e["label"],e["key"]])).collect::<Vec<_>>()});
    value["revision"] = json!(format!(
        "{:x}",
        Sha256::digest(signature.to_string().as_bytes())
    ));
    value
}
pub fn screen_identity(menu: &Value) -> String {
    menu["text"]
        .as_str()
        .unwrap_or("")
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Exact visible labels only. Ambiguity must never turn into a guessed key.
pub fn resolve(menu: &Value, label: &str) -> Result<u8, String> {
    let entries = menu["entries"]
        .as_array()
        .ok_or("Menu entries unavailable")?;
    let soft = menu["soft_keys"]
        .as_array()
        .ok_or("Soft keys unavailable")?;
    let matches: Vec<_> = entries
        .iter()
        .chain(soft)
        .filter(|entry| entry["label"] == label)
        .collect();
    if matches.len() != 1 {
        return Err("Label is absent or ambiguous on the current screen".into());
    }
    matches[0]["encoder"]
        .as_u64()
        .and_then(|v| u8::try_from(v).ok())
        .ok_or("Invalid menu encoder".into())
}

/// Arrow-list behavior is scoped to the same recognized firmware lists as Android.
/// Return whether this key enters the target (rather than moving the selection).
pub fn select(menu: &Value, label: &str) -> Result<(u8, bool), String> {
    if let Ok(code) = resolve(menu, label) {
        return Ok((code, true));
    }
    if menu["list_kind"].as_str().unwrap_or("").is_empty() {
        return Err("No unique visible function/soft-key entry with that exact label".into());
    }
    let rows = menu["list_entries"]
        .as_array()
        .ok_or("Menu rows unavailable")?;
    let matching: Vec<_> = rows
        .iter()
        .filter(|row| row["label"] == label || row["text"] == label)
        .collect();
    if matching.len() != 1 {
        return Err(
            "List entry is absent or ambiguous; only visible entries can be selected".into(),
        );
    }
    let selected = menu["selection"]["row"]
        .as_u64()
        .ok_or("Current list selection is unknown")?;
    let target = matching[0]["row"].as_u64().ok_or("List row is unknown")?;
    Ok(if target == selected {
        (16, true)
    } else if target > selected {
        (12, false)
    } else {
        (9, false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_function_keys_and_ambiguity() {
        let menu = inspect(
            "Main Menu\nF0: Diagnostics\nF4: Release Notes\n",
            &[1],
            40,
            16,
        );
        assert_eq!(resolve(&menu, "Diagnostics"), Ok(24));
        assert_eq!(menu["selection"]["text"], "F0: Diagnostics");
        assert!(resolve(&menu, "Diagnostic").is_err());
        assert!(resolve(&inspect("F0: Same\nF1: Same", &[], 40, 16), "Same").is_err());
    }
    #[test]
    fn native_cursor_changes_revision_and_unknown_stays_unknown() {
        let a = inspect("Model Year\n2004\n2005", &[1], 40, 16);
        let b = inspect("Model Year\n2004\n2005", &[2], 40, 16);
        assert_ne!(a["revision"], b["revision"]);
        assert!(inspect("Model Year\n2004", &[], 40, 16)["selection"].is_null());
    }
    #[test]
    fn footer_labels_are_observed_without_claiming_enablement() {
        let text = format!(
            "{}{}",
            "\n".repeat(14),
            "Continue  Cancel              Help      \n"
        );
        let menu = inspect(&text, &[], 40, 16);
        assert_eq!(resolve(&menu, "Cancel"), Ok(6));
        assert!(menu["soft_keys"][0]["enabled"].is_null());
    }
    #[test]
    fn visible_list_navigation_and_contract_key_parity() {
        let menu = inspect("Model Year\n2004\n2005", &[1], 40, 16);
        assert_eq!(select(&menu, "2005"), Ok((12, false)));
        assert_eq!(select(&menu, "2004"), Ok((16, true)));
        assert!(select(&menu, "2010").is_err());
        assert!(select(
            &inspect("Turn key to ON\nContinue", &[1], 40, 16),
            "Continue"
        )
        .is_err());
        let contract: Value =
            serde_json::from_str(include_str!("../ui/interface-contract.json")).unwrap();
        for (key, code) in KEYS {
            if key != "SELECT" {
                assert_eq!(contract["key_encoders"][key], code);
            }
        }
    }
    #[test]
    fn real_year_list_footer_is_selection_not_a_soft_key() {
        let text = "Vehicle Identification\nSelect one of the following\nModel Year(s)\n\n(C) 2012\n(B) 2011\n(A) 2010\n(9) 2009\n(8) 2008\n(7) 2007\n(6) 2006\n(5) 2005\n(4) 2004\n1 / 15\n(C) 2012\n\n";
        let menu = inspect(text, &[], 40, 16);
        assert_eq!(menu["selection"]["row"], 4);
        assert_eq!(menu["selection"]["source"], "guest_selected_item_footer");
        assert_eq!(menu["soft_keys"], json!([]));
        assert_eq!(select(&menu, "2012"), Ok((16, true)));
        assert_eq!(select(&menu, "2004"), Ok((12, false)));
    }
    #[test]
    fn cosmetic_redraw_is_not_a_confirmed_menu_transition() {
        let a = inspect("Main Menu\n\n F0: Diagnostics\n", &[], 40, 15);
        let b = inspect("  Main Menu  \n F0: Diagnostics \n\n", &[], 40, 16);
        assert_eq!(a["revision"], b["revision"]);
        assert_eq!(screen_identity(&a), screen_identity(&b));
        assert_ne!(
            screen_identity(&a),
            screen_identity(&inspect("Model Year\n(C) 2012", &[], 40, 15))
        );
    }
}
