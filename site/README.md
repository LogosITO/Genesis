# Site

Astro + Starlight static site. Requires Node.js 22.12 or newer.

```sh
cd site
npm ci
npm run dev
npm run build
```

Root `docs/` is the canonical technical source. `npm run build` copies it into an ignored Starlight content directory; edit the root documents, not generated files. Project pages in `src/content/docs/` are short introductions, not second technical specifications.

For this GitHub Pages project path, set `BASE_PATH=/Genesis/` when building. The Pages workflow derives this from `GITHUB_REPOSITORY`. Without it the site builds at `/` for local preview.

The site builds English (`/Genesis/`) and Russian (`/Genesis/ru/`) pages. Project pages use a wide layout without sidebars; the technical reference retains its navigation and is currently written in English. Russian reference routes display that original with an untranslated-content notice. Edit root `docs/` for technical content and the two locale folders under `src/content/docs/` for project-page copy.
