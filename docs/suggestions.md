# Another cut for one picture

When a box is not quite right (it runs onto a white border, misses a card's margin, is a little crooked), the review canvas can offer the other cuts the detectors had in mind. ScanSplitter keeps only the winner of each print and throws the rest away; this keeps them.

## Where the candidates come from

`crates/oxicut-core/src/suggest.rs`, `proposals()`, for the whole scan, whatever the detector chosen:

- v3, the classic detector (the picture only);
- the whole-card pass (`detect_cards`), when the scan looks like cards on a light lid;
- v3 with the colour threshold capped (v7's second set);
- the chroma proposals;
- with the full v7 and both MobileSAM files downloaded, v5's anchors.

Each is kept as found and moved onto the picture's own edge (`tighten_rect`), and scored with v7's brightness-step score (`score_rect`). The scan is analysed once; the app keeps the candidates of the last three scans in memory (`engine::suggest_boxes`), so the next picture on the same scan answers at once. Nothing is saved in the project.

## Which ones are offered

`suggestions_for()` takes the box on screen and keeps a candidate when it is about the same print:

- the smaller of the two is at least 70% inside the other;
- their centres are within 35% of the larger one's longer side (a box over two neighbouring prints has its centre in the gap, a card has its artwork about in the middle);
- the area is between 0.3 and 3.5 times (a card cut whole is a few times its artwork).

The user's trim is applied as for a detected box; copies of the box on screen (overlap 0.95) and of each other (0.9) are dropped; the best score comes first, at most six. Each suggestion gets the quarter turn whose angle is nearest the box on screen, so a picture turned by hand or by the orientation helper keeps its turn.

## On the screen

Selecting a box starts a quiet lookup after a short pause. The **Another cut** button pulses while it runs, then shows a count (or stays grey when there is nothing). Pressing it steps through the cuts on the canvas with a plain reason from `app/src/suggestions.ts` ("Cuts closer", "Cuts wider and straighter", "A different shape", "Moved a little"); **Use this** commits the shown box as one undo step, **Cancel** or **Esc** puts the old one back. With "reduce motion" on in Windows the pulse and the fade-in of the count are off.

## Measured

On the eight white-lid postcards with *Only the picture* shown (the classic detector's boxes), a cut closer to the owner's hand-fixed box was offered for 6 of 8 cards (for example 0.31 to 0.96 overlap, 0.37 to 0.97, 0.24 to 0.72); of the other two, one was already exact and one had nothing better (`another_cut_is_offered_that_is_closer_to_the_owners_card` in `crates/oxicut-core/tests/postcards.rs`, skipped without the private scan).

## Limits and later

- A box over two neighbouring prints can be offered when a detector made one; the preview shows it before it is used.
- Whole-scan alternatives, MobileSAM's multi-mask decoder (three masks per print, a second download) and recording which suggestion was used (the Statistics item) are not done.
