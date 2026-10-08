import { readdir, readFile, mkdir, rm, writeFile } from 'node:fs/promises';
import { join, relative, dirname } from 'node:path';
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
    const content = await readFile(path, 'utf8');
    const title = content.match(/^# (.+)$/m)?.[1] ?? entry.name;
    await mkdir(dirname(destination), { recursive: true });
    await writeFile(destination, `---\ntitle: ${JSON.stringify(title)}\n---\n\n${content}`);
  }
}
await copy(source);
