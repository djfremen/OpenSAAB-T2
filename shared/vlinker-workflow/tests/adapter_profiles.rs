// SPDX-License-Identifier: MPL-2.0
use opensaab_vlinker_workflow::{adapter_identity::Model, vlinker::COMMANDS};

#[test]
fn each_model_keeps_the_reviewed_startup_and_distinct_firmware_qualification() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/mic_startup_before_organization.json"
    ))
    .unwrap();
    for model in [Model::VlinkerMcPlus, Model::CaristaEvo] {
        assert_eq!(
            serde_json::json!(model.profile().startup),
            fixture["commands"]
        );
        // Detection can run before model selection; the three initial commands agree.
        assert_eq!(&model.profile().startup[..3], &COMMANDS[..3]);
    }
    assert_ne!(Model::VlinkerMcPlus.id(), Model::CaristaEvo.id());
    assert_ne!(
        Model::VlinkerMcPlus.probe_profile_id(true),
        Model::CaristaEvo.probe_profile_id(true)
    );
    for firmware in ["MIC3313 V2.2.92", "MIC331X", "MIC3X1X"] {
        assert!(Model::VlinkerMcPlus.probe_firmware(firmware));
        assert!(!Model::CaristaEvo.vin_firmware(firmware));
    }
    assert!(!Model::VlinkerMcPlus.vin_firmware("MIC3413 V2.0.84"));
    assert!(Model::CaristaEvo.vin_firmware("MIC3413 V2.1.0"));
    assert!(!Model::CaristaEvo.probe_firmware("MIC3413 V2.1.0"));
    assert!(Model::CaristaEvo.probe_firmware(" mic3413 v2.0.84 "));
    for product in [
        "Carista EVO+",
        "Carista EVOLUTION",
        "vLinker MCX",
        "OBDLink MX+",
    ] {
        assert!(Model::from_product(product).is_err());
    }
}
