# Social previews

`layouts/partials/social-image.html` generates a 1200 × 630 PNG for each page
with Hugo's native image filters and the bundled, OFL-licensed Space Grotesk font.
CLI pages use their command name because generated CLI Markdown has no title
frontmatter. Open Graph and Twitter share the same image and descriptive alt text.
Hugo fingerprints the generated images so updated titles get new URLs.

`background.svg` is the editable source for `background.png`; rasterize it at
1200 × 630 after changing the artwork. The PNG keeps production builds independent
of an SVG renderer. Titles and branding are drawn by Hugo, not baked into it.

To verify a production build:

```sh
hugo --gc --minify
node scripts/check-social-images.mjs public
```

The check validates matching social metadata, emitted PNG dimensions, and a
unique image for every content page. Redirect aliases are skipped.
