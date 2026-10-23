# Oxie documentation site

[oxie.bkymz.net](https://oxie.bkymz.net) is a [Starlight](https://starlight.astro.build) site built
from the Markdown in this repository. There is nothing to keep in sync by hand: edit `docs/*.md`,
`README.md` or `CHANGELOG.md`, and the site follows.

- `scripts/sync-docs.mjs` generates the pages in `src/content/docs/` (ignored by git) from
  `docs/*.md` and `CHANGELOG.md`, and the home page's "Why" and "Install" sections from the
  README. Links between docs become site links; links to other files go to GitHub. A new file in
  `docs/` must be added to the sidebar groups at the top of the script, or the build fails.
- Every ```` ```oxie ```` example gets a Rust tab with what `oxie emit` generates for it, from
  this checkout's compiler, and an Output tab with the output recorded in
  `src/data/doc-outputs.json`.
- `cargo test --workspace` runs every example with `fun main` and fails when its output differs
  from the recorded one. After checking the difference, rerecord it with
  `OXIE_BLESS=1 cargo test -p oxie --test e2e documentation_examples_run`. An example whose
  output changes from run to run is marked `// output varies`.
- Oxie code is highlighted with the VS Code extension's TextMate grammar.
- The build fails on links to missing pages or headings (`starlight-links-validator`).

## Running it

It needs Node 22.23.4 or newer (`.nvmrc` says 24) and cargo, which builds `oxie` the first time.

```bash
npm install
npm run dev
```

After changing the code block settings (`expressiveCode` in `astro.config.mjs`), restart the dev
server: pages it already rendered keep links to the old code styles until it starts afresh.

`npm run build` writes the site to `dist/`. Netlify builds it with `netlify.toml` at the root
of the repository, whenever something it is generated from changes on `main`, and builds a
preview for every pull request.
