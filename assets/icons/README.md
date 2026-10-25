# Icons

[Lucide](https://lucide.dev) 1.55.0, from `icons/` in the lucide-icons/lucide repo at that tag. They're licensed ISC, plus MIT for the icons derived from Feather; see `LICENSE`.

The only change from upstream is `stroke-width="2"` → `"1.5"`, so the icons are closer to Zed's thinner style at 16 px.

To add an icon:
1. Download `icons/<name>.svg` from the same tag and apply the same stroke change.
2. Add a variant to `ui::IconName`.
3. Add the file to `ICONS` in `src/assets.rs`.
