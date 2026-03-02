import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://amenti-labs.github.io',
  base: '/openentropy',
  integrations: [
    starlight({
      title: 'openentropy',
      logo: {
        src: './src/assets/logo.png',
        alt: 'openentropy logo',
      },
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/amenti-labs/openentropy' },
      ],
      editLink: {
        baseUrl: 'https://github.com/amenti-labs/openentropy/edit/master/website/',
      },
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { slug: 'getting-started' },
            { slug: 'getting-started/quickstart' },
          ],
        },
        { label: 'Python SDK', autogenerate: { directory: 'python-sdk' } },
        { label: 'Rust SDK', autogenerate: { directory: 'rust-sdk' } },
        { label: 'CLI', autogenerate: { directory: 'cli' } },
        { label: 'Concepts', autogenerate: { directory: 'concepts' } },
        { label: 'Guides', autogenerate: { directory: 'guides' } },
      ],
    }),
  ],
});
