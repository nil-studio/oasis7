# Six-View Robot Voxel Pipeline

This prototype converts a 2x3 six-view robot reference image into a 1 cm voxel
model.

See `DESIGN.md` for the asset contract, scoring model, refinement stages, and
pixel-world rendering usage notes.

Dependency:

```bash
python3 -m pip install -r tools/voxel_asset_pipeline/requirements.txt
```

Input layout:

```text
front | back | left
right | top  | bottom
```

Scale contract:

- Robot height defaults to `200 cm`.
- Each occupied voxel is `1 cm x 1 cm x 1 cm`.
- Voxel JSON coordinates use grid indices with `+X=right`, `+Y=up`,
  `+Z=front`; metadata includes the grid-to-world transform used by OBJ.
- OBJ units are centimeters, with the model centered on X/Z and feet at Y=0.

Source image contract:

- All six cells should have equal dimensions and matching scale/framing.
- Views must be orthographic, centered, and uncropped.
- Use transparent or plain white backgrounds; avoid shadows, floor planes,
  contact pads, labels, and perspective camera drift.
- Top view looks down at the robot head/shoulders; bottom view looks
  up from below the feet.

Outputs:

- `robot_1cm_voxel.obj`: surface mesh of exposed voxel faces.
- `robot_1cm_voxel.mtl`: quantized per-face RGB materials sampled from the input views.
- `robot_1cm_voxels.json.gz`: full occupied voxel coordinate and per-face color list.
- `robot_1cm_voxel_metadata.json`: dimensions, voxel count, mask boxes, limits.
- `robot_1cm_source_validation.json`: source cleanup/validation report.
- `robot_1cm_voxel_preview.png`: quick isometric preview.
- `robot_1cm_mask_debug.png`: the six foreground masks used for carving.
- `robot_1cm_projection_render.png`: six orthographic views re-rendered from
  the voxel model.
- `robot_1cm_projection_compare.png`: per-view source/render comparison.
- `robot_1cm_color_error.png`: color error heatmap over overlapping source/render pixels.
- `robot_1cm_projection_contact_sheet.png`: source, render, overlay, and color
  error grids stacked for art review.
- `robot_1cm_projection_metrics.json`: shape IoU/precision/recall and color
  MAE/RMSE for each view.
- `robot_1cm_refine_history.json`: optional multi-round refinement history.
- `robot_1cm_refine_round_XX_compare.png`: optional per-round comparison overlays.

Example:

```bash
python3 tools/voxel_asset_pipeline/six_view_to_voxel_model.py \
  --input /path/to/six_view_robot.png \
  --out-dir /tmp/oasis7-robot-voxel \
  --height-cm 200
```

Optimization:

```bash
python3 tools/voxel_asset_pipeline/six_view_to_voxel_model.py \
  --input /path/to/six_view_robot.png \
  --out-dir /tmp/oasis7-robot-voxel-optimized \
  --height-cm 200 \
  --optimize-fit \
  --refine-iterations 3 \
  --view-weight front=1,back=1,left=1,right=1,top=0.45,bottom=0.45 \
  --optimize-objective balanced \
  --coarse-grow-block-cm 4 \
  --source-cleanup-policy robot
```

Strict carving keeps only voxels that satisfy all six source masks. With
`--optimize-fit`, the script evaluates `6`, `5`, and `4` view-vote thresholds
and keeps the candidate with the best weighted objective score. The objective
uses shape IoU, recall, precision, and color MAE. `--optimize-objective` can
favor a balanced score, recall, or precision. `--view-weight` lets unstable
views such as top/bottom influence the score less without ignoring them.

Refinement:

`--refine-iterations` runs an additional render-compare-optimize loop after the
initial carve/threshold search. Each round tests both directions:

- `prune`: remove exposed surface voxels that project outside the source masks.
- `grow`: add connected boundary voxels when they have enough multi-view support.

Growth is constrained to the current model boundary, requires 6-neighbor
connectivity, ranks candidates by view support / missing weight / neighbor
count, and only evaluates a small top-N set. The second growth mode groups
eligible voxels into coarse patches, controlled by `--coarse-grow-block-cm`, so
larger contiguous gaps can be filled without evaluating the whole 1 cm search
space. Part-aware growth keeps a simple vertical band quota across feet, legs,
torso, shoulders, and head so refinement does not spend every candidate in the
same body region; use `--disable-part-aware-growth` to turn that off. A
candidate is accepted only when it improves the configured objective. The
history file records every evaluated operation, accepted round, voxel count,
score, and fit metrics. The loop stops early when no candidate improves the
objective. Round images are written as
`robot_1cm_refine_round_01_render.png`,
`robot_1cm_refine_round_01_compare.png`, and
`robot_1cm_refine_round_01_color_error.png`.

Method:

The script performs six-view silhouette carving. A voxel remains occupied only
when its projection falls inside all relevant front/back/left/right/top/bottom
masks. This gives deterministic physical consistency at the voxel level, but it
cannot recover internal parts that are not visible in the input views.

Source validation:

Before carving, top and bottom views are cleaned for likely generation artifacts:
large near-white, low-texture connected regions such as floor/contact patches or
painted background that should not be treated as robot geometry. The validation
step also applies a geometric support check: top/bottom mask pixels must be
supported by the front/back/left/right views at some height, otherwise they are
treated as physically unsupported source artifacts. The validation report records
removed pixels and fractions per view. Use `--source-cleanup-policy off` to
inspect raw source behavior. For asset classes where white/flat/supporting
geometry is legitimate, treat cleanup as an asset-class policy and compare raw
versus cleaned masks before accepting the model.

Color:

Each occupied voxel stores six independent face colors in the gzipped JSON:

```json
{
  "x": 0,
  "y": 0,
  "z": 0,
  "faces": {
    "front": [r, g, b],
    "back": [r, g, b],
    "left": [r, g, b],
    "right": [r, g, b],
    "top": [r, g, b],
    "bottom": [r, g, b]
  }
}
```

The front face samples the front source view, the back face samples the back
view, and so on. The OBJ export uses a paired MTL file with quantized per-face
materials, so a single 1 cm cube can have different colors on different exposed
faces.

Fit evaluation:

After generating the model, the script projects the voxel volume back into the
same six orthographic views. Shape fit compares silhouettes to the source masks.
The comparison overlay uses source/render overlap in the sampled color, red for
source pixels missed by the voxel projection, and blue for projected voxels
outside the source mask.

Color fit is evaluated only on pixels where both the source mask and re-rendered
model overlap. The metrics report mean absolute RGB error and RGB RMSE. The
color error heatmap uses cool colors for low RGB error and hot colors for high
RGB error.

The reported mean metrics are weighted by `--view-weight`; per-view metrics in
`robot_1cm_projection_metrics.json` remain unweighted so top/bottom failure can
still be diagnosed directly.
