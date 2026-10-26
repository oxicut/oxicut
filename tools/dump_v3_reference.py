"""Run ScanSplitter's Python v3 detector on the benchmark fixtures and dump
final boxes plus intermediate values, so the Rust port can be checked stage
by stage.

    uv run --project reference/ScanSplitter python -I tools/dump_v3_reference.py \
        reference/ScanSplitter benchmarks/golden/v3.json

Intermediates replicate the first lines of `detect_photos_v3` (detector.py);
keep them in sync if the reference changes.
"""
import json
import sys
from pathlib import Path

import cv2
import numpy as np
from PIL import Image

from scansplitter import detector as d

DETECTION_MAX_DIMENSION = 1800


def stages(image: Image.Image) -> dict:
    rgb = np.asarray(image.convert("RGB"))
    original_height, original_width = rgb.shape[:2]
    scale = min(1.0, DETECTION_MAX_DIMENSION / max(original_height, original_width))
    width = max(1, round(original_width * scale))
    height = max(1, round(original_height * scale))
    small = cv2.resize(rgb, (width, height), interpolation=cv2.INTER_AREA)

    lab = cv2.cvtColor(small, cv2.COLOR_RGB2LAB).astype(np.float32)
    backgrounds = d._v3_background_colors(lab)
    delta = np.full((height, width), np.inf, dtype=np.float32)
    for background in backgrounds:
        difference = lab - background
        delta = np.minimum(delta, np.sqrt(np.sum(difference * difference, axis=2)))
    otsu, _ = cv2.threshold(
        np.clip(delta, 0, 255).astype(np.uint8), 0, 255, cv2.THRESH_BINARY | cv2.THRESH_OTSU
    )
    foreground = delta > max(18.0, float(otsu))
    return {
        "detection_size": [width, height],
        "scale": scale,
        "backgrounds_lab": sorted(
            ([round(float(v), 3) for v in c] for c in backgrounds), key=lambda c: c[0]
        ),
        "otsu": int(otsu),
        "foreground_count": int(np.count_nonzero(foreground)),
    }


def main() -> None:
    root, out = Path(sys.argv[1]), Path(sys.argv[2])
    bench = root / "benchmarks"
    paths = sorted(
        p
        for folder in (bench / "fixtures", bench / "robustness" / "fixtures")
        for p in folder.rglob("*")
        if p.suffix.lower() in {".jpg", ".jpeg", ".png", ".tif", ".tiff"}
    )
    # Rust's JPEG decoder is not bit-identical to PIL's (libjpeg), which is enough
    # to change k-means clusters. Parity tests therefore read PIL's decoded pixels,
    # saved losslessly next to the checkout (reference/ is gitignored).
    decoded_root = root.parent / "decoded"
    results = {}
    for path in paths:
        image = Image.open(path)
        image.load()
        name = path.relative_to(root).as_posix()
        decoded_path = decoded_root / (name + ".png")
        decoded_path.parent.mkdir(parents=True, exist_ok=True)
        image.convert("RGB").save(decoded_path)
        regions = d.detect_photos_v3(image, inset=0)
        results[name] = {
            "size": list(image.size),
            **stages(image),
            "regions": [
                {
                    "cx": r.center[0],
                    "cy": r.center[1],
                    "w": r.size[0],
                    "h": r.size[1],
                    "angle": r.angle,
                }
                for r in regions
            ],
        }
        print(path.name, len(regions), file=sys.stderr)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"scansplitter_version": "0.8.0", "images": results}, indent=1))
    print(f"wrote {len(results)} images to {out}", file=sys.stderr)


if __name__ == "__main__":
    main()
