import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightDocSearch from '@astrojs/starlight-docsearch';

const algoliaAppId = process.env.ALGOLIA_APP_ID;
const algoliaApiKey = process.env.ALGOLIA_SEARCH_API_KEY;
const algoliaIndexName = process.env.ALGOLIA_INDEX_NAME;
const algoliaAskAiAssistantId = process.env.ALGOLIA_ASK_AI_ASSISTANT_ID;

const hasDocSearchConfig = Boolean(algoliaAppId && algoliaApiKey && algoliaIndexName);
const hasPartialDocSearchConfig =
  Boolean(algoliaAppId) || Boolean(algoliaApiKey) || Boolean(algoliaIndexName);

if (hasPartialDocSearchConfig && !hasDocSearchConfig) {
  console.warn(
    '[docs] Partial Algolia DocSearch configuration detected. Set ALGOLIA_APP_ID, ALGOLIA_SEARCH_API_KEY, and ALGOLIA_INDEX_NAME together to enable AI search.'
  );
}

const plugins = hasDocSearchConfig
  ? [
      starlightDocSearch({
        appId: algoliaAppId,
        apiKey: algoliaApiKey,
        indexName: algoliaIndexName,
        ...(algoliaAskAiAssistantId ? { askAi: algoliaAskAiAssistantId } : {}),
      }),
    ]
  : [];

export default defineConfig({
  site: 'https://amenti-labs.github.io',
  base: '/openentropy',
  integrations: [
    starlight({
      title: 'openentropy',
      plugins,
      logo: {
        src: './src/assets/logo_no_text.png',
        alt: 'openentropy logo',
      },
      favicon: '/favicon.png',
      head: [
        {
          tag: 'meta',
          attrs: { property: 'og:image', content: 'https://amenti-labs.github.io/openentropy/og-image.png' },
        },
      ],
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/amenti-labs/openentropy' },
      ],
      customCss: ['./src/styles/custom.css'],
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
