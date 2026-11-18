# Art vision

A short statement of how the game should look, for anyone making art for it. It describes the saturated style, which is the default; the classic style stays as a fallback.

## The feeling

Late afternoon on a walk you didn't plan. The light is low and golden, the shadows are long and deep, the air has a little haze in it, and the land goes on further than you can walk today. Nothing is shouting. You notice things because the world is quiet enough to notice them: a drift of lupines on a slope, wind going through a meadow in waves, a glint on a ridge that turns out to be glass.

The light and colour are those of a painted poster: a warm low sun, shadows that stay dark, and ridges that recede in flat bands of colour. Shapes are simple and clear enough to read from far off. A simple, chunky world can carry a lot when the palette and silhouettes are right, and it should feel full of life that notices you. Glassfall adds its own note: an old volcano, a field strewn with meteoric glass, and a strangeness that grows the closer you get to where it fell. Strange, never a jumble.

## Shape

- Voxels are the material, not the look. Ground slopes, canopies round off, trunks are octagons. Square edges are kept where they mean something: cliffs, built things, glass.
- Big, readable silhouettes first. A pine is a cone of level tiers, an oak is a cloud, a hill has one clear ridge line. Detail lives at the edges of a silhouette (leaf fringes, grass tips), not all over its face.
- Things have orientation. Grass grows up, conifer branches reach out level and droop, leaves sit on the outside of a crown. Anything that hangs at a random angle reads as noise.
- Scale is honest: a voxel is 25 cm, a tuft is knee-high at most, a tree is 6 to 15 m. Landmarks (a volcano, sea stacks, a glass spire) are big enough to navigate by from a kilometre away.

## Colour and light

- Warmth comes from the sun, not from paint. Surfaces keep believable colours; a golden sun and warm haze do the rest. Saturating the paint instead turns grass neon.
- Shadows are deep and slightly cool by contrast, never lifted to grey or blue-grey. Lifted shadows make the whole frame look washed out.
- Each biome has a small palette and stays inside it: meadow greens with ochre drifts, prairie gold after the fire, dark firs, black sand and pale surf, the glass field's cold sheen. Variety comes from moving between biomes, calm from staying in one.
- Colour varies in broad fields tens of metres across (meadow to dry grass, lush to sparse), not per block. Per-block noise reads as tiles.
- Distance is layered: aerial haze thickens low and far, so ridges stack up as paler silhouettes. The horizon should always offer a shape worth walking to.

## Life and motion

- Something is always moving, slowly. Grass bends in gusts that roll across a meadow, branch tips bob, leaves shiver, water glints, clouds drift. One wind drives all of it, so it reads as weather rather than as animation.
- Motion is soft and large-scale. Nothing jitters, flickers or loops visibly.
- Life reacts to you: birds lift from a meadow as you cross it, grass parts around your feet, creatures keep their distance. The world should feel inhabited before you meet anything in it.
- Light is part of the motion: sun through leaves when you look toward it, motes in a shaft of light in the forest, glow moss and glass catching the last of the day.

## Detail sprites

The flat sprites standing beside blocks (tufts, flowers, leaf clusters, conifer branches) are where most of the small-scale life comes from. They should be:

- tinted from the block they grow on, so a tuft matches its ground and a sprig matches its crown, with darker bases and lighter tips;
- firm-edged shapes rather than soft gradients (they are cut out at half alpha);
- oriented like the real plant, never scattered at random angles;
- sparse outside the places they belong, generous inside them: most grass is short, a meadow is a sea of it.

`library/sprites/README.md` is the technical spec, and `library/sprites/preview.png` shows the current set.

## What to avoid

- Sameness. A thousand identical hills is the failure to avoid; every view should have one thing in it that is different on purpose.
- Clutter. If detail doesn't help a silhouette or say something about the place, leave it out.
- Theme-park strangeness. Anomalies are rare, physical and a little unsettling, and some stay unexplained.
- Interface on top of the world. Information should come from the land itself, not from markers floating in the air.
