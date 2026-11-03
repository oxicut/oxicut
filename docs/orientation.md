# Turning pictures the right way up

OxiCut can turn a picture that lies sideways or upside down the right way up while the pictures are found. It is on by default (Detection tab, **Turn pictures the right way up**) and needs a one-time download of a helper file (about 80 MB), offered right under the switch and in the Models tab. Without the file nothing is turned and detection works as before.

## The model

- `orientation_model_v2.onnx`, an EfficientNetV2 classifier from [duartebarbosadev/deep-image-orientation-detection](https://github.com/duartebarbosadev/deep-image-orientation-detection) (MIT licence), release v2. The same file ScanSplitter uses; the registry (`crates/oxicut-models`) holds both download URLs and the SHA-256. Size 80,578,424 bytes.
- Input `[batch, 3, 384, 384]`, output four scores. Class 0, 1, 2, 3 is the number of quarter turns clockwise that makes the picture upright.
- Preprocessing is ScanSplitter's (`rotator._preprocess_for_orientation`): RGB, squash to 416 x 416 with Pillow's bilinear filter (`pil::resize_bilinear`), centre crop 384, divide by 255, ImageNet mean and standard deviation. The Rust and Python runs agree to 0.0001 in every probability on 24 cases (`tools/dump_orientation_reference.py`, `benchmarks/golden/orientation.json`, `crates/oxicut-core/tests/golden_orientation.rs`).

## Where it runs

In `analyze_scan` (`app/src-tauri/src/engine.rs`), after the boxes are found, for every detector. Each box is cut out the way the Crops view and the export cut it, the model is asked, and a box that needs a turn is turned the way the Left and Right buttons turn it (`orient::turn_rect`, which mirrors `app/src/cropTurn.ts`). The box angle is the only place a crop's turn is stored, so undo, export and saving work as for a hand turn. The model is loaded once for the whole app and shared by the detection workers (`orient::with_orientation`).

The scan keeps a note of which boxes were turned (`ScanAnalysis.turned`, saved in the project as `autoTurned`, by position). The Crops view shows a small **Turned** mark on them. The mark only counts while the box is as the automatic turn left it, so turning, resizing or moving the crop by hand removes it.

## Where it differs from ScanSplitter

ScanSplitter takes the model's single answer for a crop. On eight real postcards (each turned 0, 90, 180 and 270 degrees, 32 cases) that answer was right 26 times and turned two upright art cards upside down with high confidence.

OxiCut asks the model for all four quarter turns of the crop and combines them. A crop turned `j` quarters that gets answer `c` needs `(c + j) mod 4`. The log-probabilities are added and turned back into a probability, and the crop is only turned when the best answer has a confidence of at least 0.5 (`orient::combine`, `MIN_CONFIDENCE`). On the same 32 cases this is right 28 times and makes no confident mistake. The four misses are all one ambiguous card (confidence 0.36), which is left as it is. The cost is four model runs per picture (about 0.14 s).

Test: `postcards_are_turned_the_right_way_up_and_never_the_wrong_way` in `crates/oxicut-core/tests/postcards.rs` (skipped without the private postcard scan or the model).

ScanSplitter's fallbacks when the model fails (face detection, a Hough check) are not ported; a failure simply turns nothing.

## Limits

- Up versus down is a guess for pictures with no text, faces, horizon or other clue, such as abstract art; those are left alone when the model is not sure.
- The squash and centre crop drop the outer edge and distort very elongated pictures (the same as ScanSplitter).
- The claimed accuracy (98.82%) is for photographs; postcards were measured here as above.
