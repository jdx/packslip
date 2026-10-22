# Social previews

Every page on packslip.dev except the 404 page gets its own 1200 × 630 PNG
link preview. Read this before you change the preview's artwork, text, or font.

## How the images are built

The [social-image partial](../../layouts/partials/social-image.html) builds
each image with Hugo's native image filters and the bundled, OFL-licensed
[Space Grotesk font](fonts/README.md). It draws the page title,
"packslip / docs", and "packslip.dev" over `background.png`. The background
holds only the color, the red top bar, the divider line, and the logo mark.

Each page shows its page title, and the partial names the CLI pages itself:

- The CLI landing page shows "CLI reference".
- Each command page shows `packslip <command>`, taken from its source file's
  name.

Longer titles use smaller text. Titles longer than 260 characters are
truncated.

Open Graph and Twitter use the same image and the same alt text. The alt
text is the same title, untruncated, followed by ` — packslip`, such as
`packslip verify — packslip`. On CLI command pages it differs from the
page's HTML `<title>`, which ends in ` — CLI reference`. Hugo derives each
image's URL from the image and its filters, so a change to the rendered
title produces a new URL.

## Update the artwork

1. Edit `background.svg`, the source for the background.
2. Rasterize it as `background.png` at exactly 1200 × 630.
3. [Check the output](#check-the-output) and commit both background files.

The repository keeps the rasterized PNG so production builds run without an
SVG renderer. Text belongs in the partial, not the artwork, so each page can
have its own title.

## Check the output

Build the site, then check the generated images. Run both commands from the
repository root:

```sh
mise run docs:build
node scripts/check-social-images.mjs public
```

The check compares each page's Open Graph and Twitter metadata. It confirms
that every referenced PNG exists at 1200 × 630 and that each page has a
distinct image. It skips HTML files without an image: the 404 page and Hugo's
redirect aliases.

Neither `mise run docs:check` nor CI runs this check. Run it yourself after
you change the partial, the background artwork, the bundled font, or how page
titles are generated. It needs Node.js, which `mise install` does not provide.
