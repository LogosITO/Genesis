import { readdir, readFile, mkdir, rm, writeFile } from 'node:fs/promises';
import { join, relative, dirname, resolve, isAbsolute, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const source = fileURLToPath(new URL('../../docs/', import.meta.url));
const target = fileURLToPath(new URL('../src/content/docs/reference/', import.meta.url));
await rm(target, { recursive: true, force: true });
await mkdir(target, { recursive: true });
async function copy(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) { await copy(path); continue; }
    if (!entry.name.endsWith('.md')) continue;
    const destination = join(target, relative(source, path));
    const content = (await readFile(path, 'utf8')).replace(/\]\(((?!https?:|mailto:|#|\/)[^)#]+\.md)(#[^)]+)?\)/g, (_match, link, fragment = '') => {
      const referenced = relative(source, resolve(dirname(path), link));
      if (referenced.startsWith('..') || isAbsolute(referenced)) throw new Error(`Documentation link escapes docs/: ${link}`);
      const route = referenced.replace(/\.md$/, '').split(sep).join('/');
      return `](${process.env.BASE_PATH || '/'}reference/${route}/${fragment})`;
    });
    const title = content.match(/^# (.+)$/m)?.[1] ?? entry.name;
    await mkdir(dirname(destination), { recursive: true });
    await writeFile(destination, `---\ntitle: ${JSON.stringify(title)}\n---\n\n${content}`);
  }
}
await copy(source);
