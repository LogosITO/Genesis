import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  base: process.env.BASE_PATH || '/',
  integrations: [starlight({
    title: 'Project Name TBD',
    description: 'Experimental research on mathematically defined mutable worlds',
    social: [],
    sidebar: [
      { label: 'Project', items: ['vision', 'architecture', 'documentation', 'research', 'roadmap', 'contributing'] },
      { label: 'Technical reference', items: [{ autogenerate: { directory: 'reference' } }] },
    ],
  })],
});
