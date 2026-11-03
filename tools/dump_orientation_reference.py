"""Run ScanSplitter's Python orientation model on benchmark pictures turned four ways and dump what it
says, so the Rust port (preprocessing and model run) can be checked against it.

    uv run --offline --project reference/ScanSplitter python -I tools/dump_orientation_reference.py \
        reference/ScanSplitter benchmarks/golden/orientation.json

Needs the model: `oxicut models download orientation`. For each picture and each clockwise turn
(0, 90, 180, 270) it writes the model's four probabilities (softmax of the logits), the way
`rotator._preprocess_for_orientation` feeds it. Keep the picture list in step with the Rust test.
"""
import json
import os
import sys
from pathlib import Path

import numpy as np
import onnxruntime as ort
from PIL import Image

from scansplitter import rotator

PICTURES = [
    "benchmarks/fixtures/scansplitter-03-tape-and-photo-corners.jpg",
    "benchmarks/fixtures/scansplitter-05-narrow-mounted-gutters.jpg",
    "benchmarks/fixtures/scansplitter-10-irregular-real-world-layout.jpg",
    "benchmarks/fixtures/album-01-cream-single.jpg",
    "benchmarks/fixtures/album-05-portrait-page.jpg",
    "benchmarks/fixtures/album-09-landscape-single.jpg",
]


def main(reference: Path, out: Path) -> None:
    model = Path(os.environ["APPDATA"]) / "oxicut" / "models" / "orientation_model_v2.onnx"
    session = ort.InferenceSession(str(model), providers=["CPUExecutionProvider"])
    name = session.get_inputs()[0].name
    cases = []
    for picture in PICTURES:
        image = Image.open(reference / picture).convert("RGB")
        for turn in (0, 90, 180, 270):
            turned = image.rotate(-turn, expand=True) if turn else image  # clockwise
            x = rotator._preprocess_for_orientation(turned)
            logits = session.run(None, {name: x})[0][0]
            e = np.exp(logits - logits.max())
            cases.append(
                {
                    "picture": picture,
                    "clockwise": turn,
                    "size": list(turned.size),
                    "probabilities": [float(v) for v in e / e.sum()],
                    "answer": int(logits.argmax()),
                }
            )
    out.write_text(json.dumps({"cases": cases}, indent=1), encoding="utf-8")
    print(f"{len(cases)} cases -> {out}")


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
