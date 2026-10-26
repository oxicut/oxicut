"""Dump ScanSplitter's MobileSAM stage so the Rust port can be checked.

    reference/ScanSplitter/.venv/Scripts/python.exe -I tools/dump_sam_reference.py \
        reference/ScanSplitter benchmarks/golden/sam.json <models dir>

For a few fixtures: the encoder embedding's statistics, then for each v3 proposal the
decoder's predicted IoU, the mask's pixel count and bounding box, and the rectangle
`_v4_refine_rectangles` settles on (full-resolution coordinates).
"""
import json
import sys
from pathlib import Path

import cv2
import numpy as np
import onnxruntime
from PIL import Image

from scansplitter import detector, detector_v7 as v7

FIXTURES = [
    "benchmarks/fixtures/scansplitter-05-narrow-mounted-gutters.jpg",
    "benchmarks/fixtures/scansplitter-03-tape-and-photo-corners.jpg",
    "benchmarks/robustness/fixtures/dev/robust-dev-flatbed-white-01.jpg",
    "benchmarks/robustness/fixtures/dev/robust-dev-album-01.jpg",
]


def rect_json(rect) -> dict:
    (cx, cy), (w, h), angle = rect
    return {"cx": float(cx), "cy": float(cy), "w": float(w), "h": float(h), "angle": float(angle)}


def main() -> None:
    root, out, models = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    providers = ["CPUExecutionProvider"]
    encoder = onnxruntime.InferenceSession(str(models / "mobile_sam_image_encoder.onnx"), providers=providers)
    decoder = onnxruntime.InferenceSession(str(models / "sam_mask_decoder_single.onnx"), providers=providers)
    detector._mobilesam_encoder_session = encoder
    detector._mobilesam_decoder_session = decoder

    masks = []
    original_mask_rect = detector._v4_mask_rect

    def recording_mask_rect(binary_mask, proposal, predicted_iou):
        ys, xs = np.nonzero(binary_mask)
        masks.append(
            {
                "predicted_iou": predicted_iou,
                "mask_pixels": int(binary_mask.sum()),
                "mask_bbox": [int(xs.min()), int(ys.min()), int(xs.max()), int(ys.max())] if len(xs) else None,
            }
        )
        return original_mask_rect(binary_mask, proposal, predicted_iou)

    detector._v4_mask_rect = recording_mask_rect

    results = {}
    for name in FIXTURES:
        image = Image.open(root / name)
        image.load()
        rgb = np.asarray(image.convert("RGB"))
        height, width = rgb.shape[:2]
        proposals = v7._v3_rects(rgb, 0.02, 0.80, None)

        scale = min(1.0, 1024 / max(height, width))
        resized = cv2.resize(
            rgb, (max(1, round(width * scale)), max(1, round(height * scale))), interpolation=cv2.INTER_AREA
        )
        embedding = encoder.run(None, {"input_image": resized.astype(np.float32)})[0]

        masks.clear()
        refined = detector._v4_refine_rectangles(rgb, proposals)
        results[name] = {
            "size": [width, height],
            "scale": scale,
            "embedding_shape": list(embedding.shape),
            "embedding_sum": float(embedding.astype(np.float64).sum()),
            "embedding_head": [float(v) for v in embedding.ravel()[:16]],
            "proposals": [rect_json(r) for r in proposals],
            "masks": list(masks),
            "refined": [rect_json(r) for r in refined],
        }
        print(name, len(proposals), file=sys.stderr)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"scansplitter_version": "0.8.0", "images": results}, indent=1))


if __name__ == "__main__":
    main()
