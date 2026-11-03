# Memory use and speed

Measured on 2026-11-03 on the release build (`cargo tauri build --no-bundle`, version 0.0.1 plus the orientation helper and "Another cut"). This is raw material for the README and the website's requirements line; nothing here is written for readers yet.

## How it was measured

- **Machine:** Intel Core i7-13700K (16 cores, 24 threads), 64 GB of memory, Windows 11. A fast machine: timings on a modest laptop will be worse, memory should be about the same.
- **Scans:** `reference/postcards/postcards-8-cream-on-white-lid.jpg` (2550 x 3509, 9 megapixels, 8 prints), and the same scan enlarged to 4960 x 7016 (35 megapixels, a 600 dpi A4 bed) as a JPEG and as an 8-bit TIFF.
- **One scan:** `oxicut detect <scan> --mode <mode>`, polling the process's peak working set. The engine only, no window.
- **The whole app:** six copies of the 35 MP JPEG opened at once with the settings `{"mode":"v7","autoOrient":true}`, polling the app's working set and the sum over the app and its WebView2 child processes for 75 seconds. Scratch data folders, so nothing of a real project was touched.
- **Fewer cores:** the command line run with the processor affinity set to 4 and to 2 logical cores. This only limits the number of cores, not their speed, so it flatters a slow CPU.

## One scan, from the command line

| Detector | 9 MP scan: peak memory | 35 MP scan: peak memory | Time on the 35 MP scan |
|---|---|---|---|
| v3 | 152 MB | 298 to 305 MB | 0.6 s |
| v7-lite (the default) | 152 MB | 298 MB | 0.6 s |
| Full v7 (MobileSAM) | 686 MB | 656 MB | 3.8 s |

The full v7 costs about the same whatever the size of the scan, because MobileSAM works on a fixed-size copy. v3 and v7-lite grow with the scan, but stay small. An 8-bit TIFF and a JPEG of the same size need the same memory.

Full v7 on the 35 MP JPEG with fewer cores:

| Cores | Time | Peak memory |
|---|---|---|
| 24 logical | 3.8 s | 656 MB |
| 4 logical | 4.3 s | 656 MB |
| 2 logical | 4.8 s | 655 MB |

## The whole app

Six 35 MP scans, full v7 with the orientation helper on:

| Moment | App | App plus WebView2 |
|---|---|---|
| Just started | 27 MB | 122 MB |
| Detecting the batch (about 15 s in) | 1.25 GB | 1.6 GB |
| Peak seen | 1.74 GB | 2.09 GB |
| Finished, the six scans loaded | 271 MB | about 660 MB |

- The batch peak comes from the number of detection workers: two for the full v7, four for the other detectors (`app/src-tauri/src/batch.rs`). Every v7 worker holds its own copy of the MobileSAM sessions, which are released again when the batch ends.
- About 390 MB of the idle figure is WebView2, the browser engine the window runs in. OxiCut cannot reduce that much.
- The orientation helper keeps its model loaded after first use (about 100 MB) and takes about a seventh of a second per picture (four model runs per picture).
- The default setup (v7-lite) never loads a model for detection, so its batches stay near the single-scan figures above times the number of workers (at most four).

## What was not measured

- **16-bit TIFF scans.** By arithmetic they add roughly 300 MB per scan being processed: the 16-bit decode (about 210 MB for 35 MP) and the 8-bit working copy (about 105 MB) exist together. My test TIFF was 8-bit.
- **Real low-end hardware.** Pinning the cores of a fast CPU does not show what an old dual-core at 2 GHz does. Guess: several times slower than the figures above for the full v7. Not measured.
- **Memory limits.** Nothing was run in a machine with 4 GB, so the notes below are estimates.
- Very large batches (hundreds of scans) and long sessions.

## What it means for a modest laptop (estimates)

- **Default setup (v7-lite):** fine on 4 GB. Quick and light.
- **Full v7 on 4 GB:** a batch peak above 2 GB, with Windows and WebView2 on top, would be tight.
- **Full v7 on 8 GB:** should be fine.

## Ideas, not done

1. Choose the number of v7 workers from free memory (one below about 8 GB, two above). That roughly halves the batch peak on small machines for a small speed cost.
2. State "8 GB recommended for the full v7; the default needs much less" in the website's requirements line.
3. Run the 0.1 candidate once in a Windows machine limited to 4 GB and 2 cores, and replace the estimates above with measurements. Include a 16-bit TIFF.
4. Revisit this file when the README and website get their requirements text, and pull the key figures from it.
