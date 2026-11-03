//! Both ONNX models in one process: ONNX Runtime allows one environment per process, so MobileSAM and
//! the orientation model must share it. (They once built one each, and whichever loaded second failed
//! with "only one environment is allowed per process".) Skips when either model is not downloaded.

use std::path::PathBuf;

use oxicut_core::orient::OrientationModel;
use oxicut_core::sam::MobileSam;

fn models_dir() -> Option<PathBuf> {
    std::env::var_os("OXICUT_MODELS_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("oxicut").join("models")))
}

#[test]
fn both_models_load_in_one_process_in_either_order() {
    let Some(dir) = models_dir() else {
        eprintln!("skipping: no models folder");
        return;
    };
    let files = [
        dir.join("mobile_sam_image_encoder.onnx"),
        dir.join("sam_mask_decoder_single.onnx"),
        dir.join("orientation_model_v2.onnx"),
    ];
    if files.iter().any(|f| !f.is_file()) {
        eprintln!("skipping: the models are not all downloaded");
        return;
    }
    OrientationModel::load(&files[2]).expect("the orientation model loads first");
    MobileSam::load(&files[0], &files[1]).expect("MobileSAM loads after it");
    OrientationModel::load(&files[2]).expect("and the orientation model again");
}
