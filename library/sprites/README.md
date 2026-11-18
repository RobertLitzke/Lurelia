# Sprites

Flat images the game stands beside blocks to make the world look alive: grass tufts and wildflowers on grassy ground, leaf clusters and pine sprigs around canopies. They sway in the wind. Physics never sees them; digging a block removes what grew on it.

Each PNG here replaces the built-in drawing of the same name. Delete a PNG to go back to the built-in one. `sheet.png` shows the set as stored, over grey; `preview.png` shows it tinted roughly as the game does, over sky and meadow. Both are only for looking at; the game never reads them.

## Files

| File | Kind | Grows on | Tinted |
|---|---|---|---|
| `tuft_short.png` | card | grass outside meadows, forest floor | yes |
| `tuft_tall.png` | card | meadows | yes |
| `tuft_seed.png` | card | meadows, the tallest patches | yes |
| `flowers_white.png` | card | meadows, in drifts | no |
| `flowers_blue.png` | card | meadows, in drifts | no |
| `leaves_a.png` | billboard | broadleaf, birch and autumn canopies | yes |
| `leaves_b.png` | billboard | the same, smaller leaves | yes |
| `needles.png` | billboard | pine and conifer canopies | yes |
| `frond.png` | frond | fern marker voxels (plant form `herb`) | yes |
| `fireweed.png` | card | fireweed marker voxels (plant form `herb`) | no |
| `heather.png` | card | heather marker voxels (plant form `herb`) | no |
| `glassbloom.png` | card | glassbloom marker voxels (plant form `herb`) | no |
| `sage.png` | billboard | the sage crown of sagebrush, and tamarisk crowns | yes |
| `tuft_long.png` | card | long-grass patches inside meadows | yes |
| `rabbitbrush.png` | card | rabbitbrush marker voxels (plant form `herb`) | no |
| `tuft_patch.png` | card | grass beyond 26 m, one card per 2 m patch | yes |
| `flowers_yellow.png` | card | prairie grass, in patches and fields | no |
| `flowers_red.png` | card | prairie grass (scarlet paintbrush), in patches and fields | no |
| `huckleberry.png` | card | huckleberry marker voxels (plant form `herb`) | no |
| `thimbleberry.png` | card | thimbleberry marker voxels (plant form `herb`) | no |

## Format

- Square RGBA PNG, 128, 256 or 512 px. The game keeps every layer at 256: a 128 image is scaled up (the grasses, flowers and leaves were drawn at 128), a 512 one box-filtered down. Any other size is ignored with a message on the console.
- sRGB colour, straight alpha (not premultiplied). Colour under fully transparent pixels doesn't matter.
- Edges are cut at alpha 0.5 and softened over about one screen pixel (alpha to coverage under MSAA). Pixels below half alpha disappear, so soft glows and long feathered gradients don't survive; paint shapes with firm edges. Keep strokes at least 2 px wide at 128, or they break up in the distance.

## Cards (tufts and flowers)

- The plant grows up from the bottom edge of the image; its roots touch it. Nothing should hang below the root.
- Grass tufts root across most of the width (the built-in ones over the middle 70%, crowded toward the centre, the outer blades shorter and leaning out), so a tuft reads as a mound and neighbours close the gaps between them. The shader runs over the whole card whether a pixel is blade or empty, so filling the card costs nothing extra.
- Close to the camera a meadow grows up to three more tufts per column than it shows from afar (`FADE_NEAR` in `src/mesh/sprites.rs`); they shrink away between 27 and 42 m. Grass bases (not herbs or big plants) shade toward the understory colour that `understory()` in `src/shaders/scene.wgsl` paints on grass ground within 55 m.
- `tuft_patch.png` stands in for all the tufts of a 2 m patch of grass from 26 m out (tufts shrink away between 32 and 46 m, patches between 95 and 115 m). It is shown about four times as wide as tall, so draw it squeezed sideways: a strip of blades along the bottom with an uneven top edge, shorter toward both ends so the card has no square corners. It is only ever a few pixels tall, so coverage and outline matter, not single blades. `VOXEL_GRASS_PATCHES=0` turns them off for comparisons.
- `tuft_long.png` is long grass, 1.1 to 1.6 m tall and shown 0.75 as wide as tall, so the drawing is squeezed sideways; draw it at 256 or 512. The built-in one has long blades arching over and a few seed stalks with drooping plumes.
- In the game a card is 0.3 to 0.65 m wide and 0.25 to 0.68 m tall, so the image is shown at roughly its own aspect, sometimes squashed to about 0.9:1. Two cards stand crossed at right angles, so the plant is seen from every side.
- Wind bends the top. The bend grows with the square of the height in the image, so the bottom quarter barely moves and the tips move most. Long thin blades read best.
- The game adds no shading of its own beyond sun, shadow and fog: darken the base (the built-in tufts go from 42% at the root to 100% at the tips) so tufts sit in the ground instead of floating on it.

## Billboards (leaves and needles)

- Centred. A billboard always faces the camera and is rolled to a random angle, so there is no up or down. Keep the shape inside the inscribed circle; corners get seen at every angle.
- In the game a billboard is 0.45 to 0.8 m across, centred just outside a leafy block's face, so roughly half of it is hidden inside the crown. The outer half is what feathers the canopy's silhouette. Ragged, leafy outlines matter more than the middle.
- Wind shakes the whole billboard a little and rolls it slightly.

## Herbs (ferns and fireweed)

A plant with growth form `herb` puts one voxel of its leaf material on the ground and nothing else; the mesher draws no faces for it, and the sprite pass grows the whole plant from it (`Rule::Herb` in `src/mesh/sprites.rs`; physics still treats it as a 0.25 m block).

- `frond.png`: one frond seen from above, root at the bottom centre, tip at the top edge, like `needles.png`. A fern is 8 to 12 fronds round the root, each 0.8 to 1.25 m long and 0.32 of that wide. The shader stands each one up at 63 to 84 degrees and curls it over toward the tip (`kind == 3u` in foliage.wgsl). Painted in grey and tinted by the fern block's colour; keep it a little under 77% grey or a fern in sun goes lime.
- `fireweed.png`: a clump of stems on two crossed cards, root at the bottom centre, shown 0.6 as wide as tall (1 to 1.6 m tall), so the drawing is stretched sideways by 1/0.6. Real colours. Two or three clumps per plant.
- `heather.png`: a low clump, shown 1.4 as wide as tall (0.35 to 0.65 m), four to six clumps run together into a mat. Real colours.
- `glassbloom.png`: a fan of faceted glass blades, shown 0.9 as wide as tall (0.9 to 1.4 m), two or three clumps. Real colours.
- `huckleberry.png` and `thimbleberry.png`: forest-floor bushes of arching leafy stems, shown 1.25 and 1.2 as wide as tall (0.75 to 1.25 m), three to five clumps run together. Real colours, kept near the canopy greens. You walk through them.
- Each clump's width, height and count are in `library_rule` in `src/mesh/sprites.rs`. Herb cards fade out at leaf distance (70 to 92 m), not grass distance.

## Tinting

Tinted images are painted in grey. The game multiplies each pixel by the colour of the block the sprite grows from (its texture's average) and by 1.3, so:

- white = 1.3 x the block's colour, a sunlit tip;
- about 77% grey = the block's colour, the same as the ground or crown beside it;
- darker = shade.

A slight warm or cool cast is fine (it multiplies too), but strong colours fight the block colours, which change between biomes and between the classic and calm styles. Flowers are not tinted: paint them in their real colours, stems included.

## Checking your work

Run the game (`cargo run --release`) and walk into a meadow (Glassfall's start is next to one), or take a headless shot:

```bash
cargo run --release -- --headless --style calm --no-hud --screenshot shots/sprites_check.png
```

To redraw `sheet.png` and `preview.png` from the current files (existing sprite PNGs are left alone):

```bash
cargo test --release export_sprites -- --ignored
```

`SPRITES_OVERWRITE=1` in front of that replaces every PNG with the built-in drawing.

## Adding a new sprite

A new image needs a few lines of code: a layer in `src/mesh/sprites.rs` (`layer`), a file name in `src/render/sprite_tex.rs` (`NAMES`, same order), and a rule saying which blocks grow it (`rule` and `library_rule` in `sprites.rs`).
