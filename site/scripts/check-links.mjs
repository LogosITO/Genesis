import { readdir, readFile, access } from 'node:fs/promises';
import { join, relative, sep } from 'node:path';

const root = 'dist';
const base = process.env.BASE_PATH || '/';
let checked = 0;
async function walk(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) { await walk(path); continue; }
    if (!path.endsWith('.html')) continue;
    const route = relative(root, path).split(sep).join('/').replace(/index\.html$/, '');
    const pageUrl = `https://example.invalid${base}${route}`;
    const html = await readFile(path, 'utf8');
    for (const [, href] of html.matchAll(/href="([^"]+)"/g)) {
      if (href.startsWith('#')) continue;
      const url = new URL(href, pageUrl);
      if (url.hostname !== 'example.invalid') continue;
      if (!url.pathname.startsWith(base)) throw new Error(`${route}: link escapes base path: ${href}`);
      const destination = decodeURIComponent(url.pathname.slice(base.length));
      const target = join(root, destination, destination.endsWith('/') || !destination ? 'index.html' : '');
      await access(target).catch(() => { throw new Error(`${route}: broken link: ${href}`); });
      checked++;
    }
  }
}
await walk(root);
console.log(`Checked ${checked} local links under ${base}`);
