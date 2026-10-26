"""Dump ScanSplitter v7's intermediate results so the Rust port can be checked.

    uv run --project reference/ScanSplitter python -I tools/dump_v7_reference.py \
        reference/ScanSplitter benchmarks/golden/v7.json

The v7 pipeline needs MobileSAM for its v5 anchors, so this mirrors
`detect_photos_v7` with the v3 proposals as the anchors instead (the same
stages, no model). Each stage is recorded in full-resolution coordinates.
"""
import json
import sys
from pathlib import Path

import cv2
import numpy as np
from PIL import Image

from scansplitter import detector_v7 as v7

WORK_MAX = 2000


def rect_json(rect) -> dict:
    (cx, cy), (w, h), angle = rect
    return {"cx": float(cx), "cy": float(cy), "w": float(w), "h": float(h), "angle": float(angle)}


def main() -> None:
    root, out = Path(sys.argv[1]), Path(sys.argv[2])
    bench = root / "benchmarks"
    paths = sorted(
        p
        for folder in (bench / "fixtures", bench / "robustness" / "fixtures")
        for p in folder.rglob("*")
        if p.suffix.lower() in {".jpg", ".jpeg", ".png", ".tif", ".tiff"}
    )
    results = {}
    for path in paths:
        image = Image.open(path)
        image.load()
        image = image.convert("RGB")
        rgb = np.asarray(image)
        width, height = image.size
        scale = min(1.0, WORK_MAX / max(width, height))
        gray = np.asarray(
            image.convert("L").resize(
                (max(1, round(width * scale)), max(1, round(height * scale))), Image.BILINEAR
            )
        ).astype(np.float32)

        v3 = v7._v3_rects(rgb, 0.02, 0.80, None)
        tight = [v7._tighten_rect(gray, rect, scale) for rect in v3]
        results[path.relative_to(root).as_posix()] = {
            "size": [width, height],
            "work_scale": scale,
            "v3": [rect_json(r) for r in v3],
            "tight": [rect_json(r) for r in tight],
        }
        print(path.name, len(v3), file=sys.stderr)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"scansplitter_version": "0.8.0", "images": results}, indent=1))
    print(f"wrote {len(results)} images to {out}", file=sys.stderr)


if __name__ == "__main__":
    main()
