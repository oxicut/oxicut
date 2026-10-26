"""Dump every stage of ScanSplitter's full v7 detector (with MobileSAM).

    reference/ScanSplitter/.venv/Scripts/python.exe -I tools/dump_v7_full_reference.py \
        reference/ScanSplitter benchmarks/golden/v7_full.json <models dir>

Mirrors `detect_photos_v7` and records, in full-resolution coordinates: the v3, capped-v3
and chroma proposals, the texture islands, closed frames, the v5 rectangle set, each
candidate's score/sharpness/ghost statistics after tightening, the arbitration result and
the final rectangles.
"""
import json
import sys
from pathlib import Path

import cv2
import numpy as np
import onnxruntime
from PIL import Image

from scansplitter import detector as d
from scansplitter import detector_v7 as v7


def rect_json(rect) -> dict:
    (cx, cy), (w, h), angle = rect
    return {"cx": float(cx), "cy": float(cy), "w": float(w), "h": float(h), "angle": float(angle)}


def main() -> None:
    root, out, models = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    providers = ["CPUExecutionProvider"]
    d._mobilesam_encoder_session = onnxruntime.InferenceSession(
        str(models / "mobile_sam_image_encoder.onnx"), providers=providers
    )
    d._mobilesam_decoder_session = onnxruntime.InferenceSession(
        str(models / "sam_mask_decoder_single.onnx"), providers=providers
    )

    bench = root / "benchmarks"
    paths = sorted(
        p
        for folder in (bench / "fixtures", bench / "robustness" / "fixtures")
        for p in folder.rglob("*")
        if p.suffix.lower() in {".jpg", ".jpeg", ".png", ".tif", ".tiff"}
    )
    min_ratio, max_ratio = 0.02, 0.80
    results = {}
    for path in paths:
        image = Image.open(path)
        image.load()
        image = image.convert("RGB")
        rgb = np.asarray(image)
        width, height = image.size
        scale = min(1.0, v7._WORK_MAX / max(width, height))
        gray = np.asarray(
            image.convert("L").resize((max(1, round(width * scale)), max(1, round(height * scale))), Image.BILINEAR)
        ).astype(np.float32)
        gray_s = cv2.GaussianBlur(gray, (0, 0), 1.0)
        gray_e = cv2.GaussianBlur(gray, (0, 0), 0.7)
        g_scale = min(1.0, 1500 / max(width, height))
        g1500 = np.asarray(
            image.convert("L").resize((max(1, round(width * g_scale)), max(1, round(height * g_scale))), Image.BILINEAR)
        ).astype(np.float32)

        v3 = v7._v3_rects(rgb, min_ratio, max_ratio, None)
        v3c = v7._v3_rects(rgb, min_ratio, max_ratio, 30.0)
        chroma = v7._chroma_rects(rgb, min_ratio, max_ratio)
        tex = d._v5_texture_rectangles(rgb, min_area_ratio=min_ratio, max_area_ratio=max_ratio)
        frames = d._v5_frame_rectangles(rgb, min_area_ratio=min_ratio, max_area_ratio=max_ratio)
        v5 = v7._v5_rectangles(image, rgb, v3, min_ratio, max_ratio)

        def prep(rects):
            items = []
            for rect in rects:
                tight = v7._tighten_rect(gray, rect, scale)
                items.append(
                    {
                        "rect": tight,
                        "score": v7._score_rect(gray_s, tight, scale),
                        "sharp": v7._edge_sharpness(gray_e, tight, scale),
                        "ghost": v7._ghost_stats(g1500, tight, g_scale),
                    }
                )
            return items

        def items_json(items):
            return [
                {
                    "rect": rect_json(i["rect"]),
                    "score": i["score"],
                    "sharp": i["sharp"],
                    "ghost": [float(i["ghost"][0]), float(i["ghost"][1])],
                }
                for i in items
            ]

        anchors, alternatives = prep(v5), prep(v3) + prep(v3c) + prep(chroma)
        chosen = v7._select(anchors, alternatives)
        area = width * height
        in_range = [
            a for a in chosen if min_ratio <= a["rect"][1][0] * a["rect"][1][1] / area <= max_ratio
        ]
        final = v7._drop_ghosts(in_range)

        results[path.relative_to(root).as_posix()] = {
            "size": [width, height],
            "v3": [rect_json(r) for r in v3],
            "v3c": [rect_json(r) for r in v3c],
            "chroma": [rect_json(r) for r in chroma],
            "texture": [{"rect": rect_json(r), "touches": bool(t)} for r, t in tex],
            "frames": [rect_json(r) for r in frames],
            "v5": [rect_json(r) for r in v5],
            "anchors": items_json(anchors),
            "alternatives": items_json(alternatives),
            "chosen": items_json(chosen),
            "final": items_json(final),
        }
        print(path.name, len(v3), len(v3c), len(chroma), len(v5), len(final), file=sys.stderr)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"scansplitter_version": "0.8.0", "images": results}, indent=1))


if __name__ == "__main__":
    main()
