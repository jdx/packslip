# Social previews

The [social-image partial](../../layouts/partials/social-image.html) generates
a 1200 × 630 PNG for each page using Hugo's native image filters and the
bundled, OFL-licensed [Space Grotesk font](fonts/README.md).

## How images are built

The partial draws the page title, "packslip / docs", and "packslip.dev" over
`background.png`. The background contains only the color, red top bar,
divider line, and logo mark.

Most pages use their page title. The CLI landing page uses "CLI reference",
and command pages use `packslip <command>` from the source file's name:
generated CLI Markdown has no title front matter. Longer titles use smaller
text, and rendered titles are truncated to 260 characters.

Open Graph and Twitter use the same image and alt text, `<title> — packslip`.
Hugo generates the image URLs from the image and its filters, so changes
to the rendered title produce a new URL.

## Update the artwork

1. Edit `background.svg`, the source for the background artwork.
2. Rasterize it as `background.png` at exactly 1200 × 630.
3. Check the built previews below and commit both background files.

Keeping the rasterized PNG in the repository lets production builds run
without an SVG renderer. Text belongs in the partial, so each page can
have its own title.

## Check the output

Build the documentation, then check the generated images:

```sh
mise run docs:build
node scripts/check-social-images.mjs public
```

The check compares Open Graph and Twitter metadata, confirms that the
referenced PNGs exist at 1200 × 630, and requires a distinct image for
each content page. It skips redirect aliases.

Neither `mise run docs:check` nor CI runs this check, so run it yourself after
changing the partial, background artwork, bundled font, or page-title
generation. The check needs Node.js, which `mise install` does not provide.
