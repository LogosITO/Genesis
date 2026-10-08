# Site

Astro + Starlight static site. Requires Node.js 22.12 or newer.

```sh
cd site
npm ci
npm run dev
npm run build
```

Root `docs/` is the canonical technical source. `npm run build` copies it into an ignored Starlight content directory; edit the root documents, not generated files. Project pages in `src/content/docs/` are short introductions, not second technical specifications.

For a GitHub Pages project path, set `BASE_PATH=/repository-name/` when building. The Pages workflow derives this from `GITHUB_REPOSITORY`. Without it the site builds at `/` for local preview. There is no configured production domain or repository identity.
