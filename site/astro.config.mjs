import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  base: process.env.BASE_PATH || '/',
  integrations: [starlight({
    title: 'Genesis',
    description: 'Experimental research on mathematically defined mutable worlds',
    locales: {
      root: { label: 'English', lang: 'en' },
      ru: { label: 'Русский', lang: 'ru' },
    },
    social: [],
    sidebar: [
      { label: 'Project', translations: { ru: 'Проект' }, items: ['vision', 'architecture', 'documentation', 'research', 'roadmap', 'contributing'] },
      { label: 'Technical reference', translations: { ru: 'Техническая документация' }, items: [{ autogenerate: { directory: 'reference' } }] },
    ],
  })],
});
