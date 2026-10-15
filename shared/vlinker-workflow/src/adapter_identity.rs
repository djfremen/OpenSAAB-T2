// SPDX-License-Identifier: MPL-2.0
//! Product identity and command profiles are separate from Bluetooth discovery names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    VlinkerMcPlus,
    CaristaEvo,
}

impl Model {
    pub fn profile(self) -> &'static crate::adapters::Profile {
        match self {
            Self::VlinkerMcPlus => &crate::adapters::vlinker_mc_plus::PROFILE,
            Self::CaristaEvo => &crate::adapters::carista_evo::PROFILE,
        }
    }
    pub fn from_product(product: &str) -> Result<Self, String> {
        [Self::VlinkerMcPlus, Self::CaristaEvo]
            .into_iter()
            .find(|model| (model.profile().matches_product)(product))
            .ok_or_else(|| format!("Unsupported Bluetooth adapter identity: {product}"))
    }
    pub fn id(self) -> &'static str {
        self.profile().id
    }
    pub fn label(self) -> &'static str {
        self.profile().label
    }
    pub fn vin_profile_id(self) -> &'static str {
        self.profile().vin_profile
    }
    pub fn probe_profile_id(self, codes: bool) -> &'static str {
        if codes {
            self.profile().dtc_profile
        } else {
            self.profile().info_profile
        }
    }
    pub fn probe_firmware(self, firmware: &str) -> bool {
        (self.profile().probe_firmware)(firmware)
    }
    pub fn vin_firmware(self, firmware: &str) -> bool {
        (self.profile().vin_firmware)(firmware)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_identity_requires_a_complete_product_name() {
        assert_eq!(
            Model::from_product("vLinker MC+ v2.2.92").unwrap(),
            Model::VlinkerMcPlus
        );
        assert_eq!(
            Model::from_product("Carista EVO v2.3.0").unwrap(),
            Model::CaristaEvo
        );
        for product in [
            "ELM327",
            "Carista OBD",
            "Carista EVOLUTION",
            "vLinker MCX",
            "vLinker FD",
            "Other Carista EVO",
        ] {
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
