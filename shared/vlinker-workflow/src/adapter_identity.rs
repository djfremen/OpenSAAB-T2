// SPDX-License-Identifier: MPL-2.0
//! Product identity and command profiles are separate from Bluetooth discovery names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model { VlinkerMcPlus, CaristaEvo }

impl Model {
    pub fn from_product(product: &str) -> Result<Self, String> {
        let upper = product.trim().to_ascii_uppercase();
        for (prefix, model) in [("VLINKER MC", Self::VlinkerMcPlus), ("CARISTA EVO", Self::CaristaEvo)] {
            if let Some(rest) = upper.strip_prefix(prefix) {
                if rest.is_empty() || rest.starts_with(char::is_whitespace)
                    || model == Self::VlinkerMcPlus && (rest == "+" || rest.starts_with("+ ")) {
                    return Ok(model);
                }
            }
        }
        Err(format!("Unsupported Bluetooth adapter identity: {product}"))
    }
    pub fn id(self) -> &'static str { match self { Self::VlinkerMcPlus => "vlinker_mc_plus", Self::CaristaEvo => "carista_evo" } }
    pub fn label(self) -> &'static str { match self { Self::VlinkerMcPlus => "vLinker MC+", Self::CaristaEvo => "Carista EVO" } }
    pub fn vin_profile_id(self) -> &'static str { match self { Self::VlinkerMcPlus => "vlinker_mc_mic_hscan_vin_v1", Self::CaristaEvo => "carista_evo_mic3413_hscan_vin_v1" } }
    pub fn probe_profile_id(self, codes: bool) -> &'static str {
        match (self, codes) {
            (Self::VlinkerMcPlus, false) => "vlinker_mc_mic_hscan_ecm_info_v1",
            (Self::VlinkerMcPlus, true) => "vlinker_mc_mic_hscan_ecm_dtc_v1",
            (Self::CaristaEvo, false) => "carista_evo_mic3413_hscan_ecm_info_v1",
            (Self::CaristaEvo, true) => "carista_evo_mic3413_hscan_ecm_dtc_v1",
        }
    }
    pub fn probe_firmware(self, firmware: &str) -> bool {
        match self {
            Self::VlinkerMcPlus => self.vin_firmware(firmware),
            // Bounded EVO ECM qualification starts with the owned firmware only.
            Self::CaristaEvo => firmware.trim().eq_ignore_ascii_case("MIC3413 V2.0.84"),
        }
    }
    pub fn vin_firmware(self, firmware: &str) -> bool {
        let upper = firmware.to_ascii_uppercase();
        let tokens: Vec<_> = upper.split(|c: char| !c.is_ascii_alphanumeric()).collect();
        match self {
            Self::VlinkerMcPlus => tokens.iter().any(|t| ["MIC3313", "MIC331X", "MIC3X1X"].contains(t)),
            // Owned EVO: VTI Carista EVO v2.2.84; VTVERS MIC3413 V2.0.84.
            // Protocol/bus readback still gates each individual VIN attempt.
            Self::CaristaEvo => tokens.contains(&"MIC3413"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_identity_requires_a_complete_product_name() {
        assert_eq!(Model::from_product("vLinker MC+ v2.2.92").unwrap(), Model::VlinkerMcPlus);
        assert_eq!(Model::from_product("Carista EVO v2.3.0").unwrap(), Model::CaristaEvo);
        for product in ["ELM327", "Carista OBD", "Carista EVOLUTION", "vLinker MCX", "vLinker FD", "Other Carista EVO"] {
            assert!(Model::from_product(product).is_err(), "{product}");
        }
        assert!(!Model::VlinkerMcPlus.vin_firmware("MIC3322 v2.3.24"));
        assert!(!Model::VlinkerMcPlus.vin_firmware("MIC3313FAKE"));
        assert!(Model::CaristaEvo.vin_firmware("MIC3413 V2.0.84"));
        assert!(!Model::CaristaEvo.vin_firmware("MIC3313 V2.0.84"));
        assert!(!Model::CaristaEvo.vin_firmware("MIC3413FAKE V2.0.84"));
        assert!(Model::CaristaEvo.probe_firmware("MIC3413 V2.0.84"));
        assert!(!Model::CaristaEvo.probe_firmware("MIC3413 V2.1.0"));
        assert!(!Model::CaristaEvo.probe_firmware("MIC3313 V2.0.84"));
    }
}
