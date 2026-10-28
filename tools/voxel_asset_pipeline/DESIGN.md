# Pixel World Voxel Asset Pipeline Design

This pipeline turns a six-view orthographic robot reference into a colored
centimeter-scale voxel asset suitable as a base for pixel-world rendering.
It is deterministic after the source image is supplied; generated preview and
evaluation files are local artifacts and should not be committed.

## Asset Contract

- Input is a single `2 x 3` image with views ordered as:

```text
front | back | left
right | top  | bottom
```

- The default real-world scale is a `200 cm` robot.
- The model voxel size is `1 cm x 1 cm x 1 cm`.
- Occupancy is stored in `robot_1cm_voxels.json.gz`.
- Each voxel stores six independent face colors: `front`, `back`, `left`,
  `right`, `top`, and `bottom`.
- OBJ output is an exposed-surface mesh in centimeter units. It uses an MTL file
  with quantized per-face materials. Hidden/internal voxel faces remain available
  in the gzipped voxel JSON, not in the OBJ surface mesh.
- Voxel JSON includes a `schema_version`, generator name, dimensions, and a
  machine-readable coordinate contract. Runtime loaders should use the JSON,
  not OBJ, when they need cube instances and per-face colors.

## Source Image Contract

- All six grid cells should have equal dimensions.
- All views should be orthographic, centered, uncropped, and share one scale.
- Background should be transparent or plain white.
- The source should avoid shadows, floor planes, contact pads, labels, and
  perspective camera drift.
- Top view looks downward from above the robot; bottom view looks upward from
  below the feet.
- If the source violates this contract, the pipeline may still produce a model,
  but the fit metrics should be treated as diagnostic rather than acceptance
  evidence.

## Pipeline Stages

1. **Grid split**
   The input image is split into the six fixed orthographic views.

2. **Foreground masks**
   Transparent inputs use alpha. Opaque inputs fall back to local-contrast
   foreground detection.

3. **Source cleanup**
   Top and bottom views receive source validation cleanup for generation
   artifacts such as floor/contact patches. A geometric support pass removes
   plan-view pixels that are unsupported by front/back/left/right masks. This is
   a robot/Image2-oriented default; future asset classes should make cleanup
   policy explicit because white, flat, or supporting geometry can be legitimate.

4. **Visual hull carving**
   The initial volume is carved from the six masks. Strict `6/6` voting produces
   the most physically conservative result. `--optimize-fit` evaluates `6`, `5`,
   and `4` view-vote candidates and keeps the best weighted objective.

5. **Color transfer**
   Each occupied voxel samples RGB from the matching source projection for every
   face. Missing samples fall back to the local average of available view colors.

6. **Render-compare feedback**
   The current voxel grid is re-rendered into the six source views. The pipeline
   computes shape IoU/precision/recall and color MAE/RMSE, then evaluates
   candidate grid changes.

7. **Bidirectional refinement**
   Refinement tests both `prune` and `grow` candidates:

   - `prune` removes exposed surface voxels that project outside source masks.
   - `grow` adds connected boundary voxels with enough multi-view support.
   - `coarse_patch` growth groups eligible 1 cm candidates into larger blocks,
     controlled by `--coarse-grow-block-cm`, so contiguous missing regions can
     be filled without searching the whole volume.
   - Part-aware growth spreads candidate budget across vertical body bands
     (`feet`, `legs`, `torso`, `shoulders`, `head`).

8. **Artifact export**
   The final grid exports voxel JSON, OBJ/MTL, preview render, mask debug, final
   six-view projection render, comparison overlay, color error heatmap, metrics,
   and refinement history.

## Objective

The weighted objective balances shape and color fit:

- Shape: IoU, precision, recall.
- Color: overlapping-pixel RGB MAE.
- View weights default to `front/back/left/right = 1.0` and
  `top/bottom = 0.45`, because generated top/bottom views are often less stable.

The default `balanced` objective accepts changes only when the weighted score
improves. `recall` and `precision` modes are available for more aggressive fill
or more conservative clipping.

Acceptance for authored assets should combine metrics with visual review. As a
starting policy, inspect the contact sheet and prefer the lowest refinement
round that materially improves recall/IoU without introducing visible blue
over-projection in important silhouette regions.

## Recommended Command

```bash
python3 tools/voxel_asset_pipeline/six_view_to_voxel_model.py \
  --input /path/to/six_view_robot.png \
  --out-dir artifacts/robot_voxel_pipeline/my_robot \
  --height-cm 200 \
  --optimize-fit \
  --refine-iterations 3 \
  --view-weight front=1,back=1,left=1,right=1,top=0.45,bottom=0.45 \
  --optimize-objective balanced \
  --coarse-grow-block-cm 4
```

For the Image 2 robot sample used during development, three refinement rounds
gave the best weighted score observed in local validation. More rounds may keep
improving recall while reducing precision, so compare the rendered overlays
before choosing a final asset.

## Reading The Outputs

- `robot_1cm_projection_render.png`: generated asset re-rendered into six views.
- `robot_1cm_projection_compare.png`: source/render overlap in sampled color,
  red for source pixels missed by the model, blue for model pixels outside the
  source mask.
- `robot_1cm_color_error.png`: RGB error on overlapping source/render pixels.
- `robot_1cm_projection_contact_sheet.png`: source, render, comparison overlay,
  and color-error rows in one image for art review.
- `robot_1cm_projection_metrics.json`: final weighted mean metrics plus per-view
  unweighted details.
- `robot_1cm_refine_history.json`: each accepted/rejected feedback candidate.

For pixel-world rendering, consume `robot_1cm_voxels.json.gz` when actual cube
instances and per-face colors are needed. Use OBJ/MTL for quick inspection,
thumbnail generation, or tooling that only understands mesh surfaces.

Cleanup policy is explicit in generated validation metadata. The default
`robot` policy is tuned for generated robot six-view sheets; use
`--source-cleanup-policy off` when evaluating asset classes where plan-view
support surfaces, flat white parts, or contact geometry may be intentional.

## Limits

- The method reconstructs a silhouette-consistent visual hull. It does not infer
  hidden internal details.
- If the six source views disagree, the pipeline can only choose a tradeoff
  between missing source pixels and over-projected model pixels.
- Current part awareness is a vertical quota heuristic, not semantic limb
  segmentation. Future work can add explicit robot part masks before carving.
- The gzipped JSON is deterministic for the same source and command, but it is
  not yet optimized as a compact runtime binary format. Future runtime work can
  add palette/index, chunking, LOD, or RLE encodings while preserving the schema
  contract.
