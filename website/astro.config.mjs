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
            { slug: 'getting-started/choose-your-path' },
            { slug: 'getting-started/quickstart' },
          ],
        },
        {
          label: 'Quick References',
          items: [
            { slug: 'cli/reference' },
            { slug: 'python-sdk/quick-reference' },
            { slug: 'rust-sdk/quick-reference' },
          ],
        },
        {
          label: 'CLI',
          items: [
            { slug: 'cli' },
            { slug: 'cli/sdk-mapping' },
          ],
        },
        {
          label: 'Python SDK',
          items: [
            { slug: 'python-sdk' },
            { slug: 'python-sdk/analysis' },
            { slug: 'python-sdk/reference' },
          ],
        },
        {
          label: 'Rust SDK',
          items: [
            { slug: 'rust-sdk' },
            { slug: 'rust-sdk/analysis' },
            { slug: 'rust-sdk/api' },
          ],
        },
        {
          label: 'Analysis & Validation',
          items: [
            { slug: 'concepts/analysis' },
            { slug: 'concepts/analysis-forensic' },
            { slug: 'concepts/analysis-entropy' },
            { slug: 'concepts/analysis-chaos' },
            { slug: 'concepts/analysis-cross-correlation' },
            { slug: 'concepts/analysis-verdicts' },
            { slug: 'concepts/trials' },
            { slug: 'guides/security-validation' },
            { slug: 'guides/research-methodology' },
          ],
        },
        {
          label: 'Concepts',
          items: [
            { slug: 'concepts/sources' },
            { slug: 'concepts/sources/timing' },
            { slug: 'concepts/sources/scheduling' },
            { slug: 'concepts/sources/system' },
            { slug: 'concepts/sources/network' },
            { slug: 'concepts/sources/io' },
            { slug: 'concepts/sources/ipc' },
            { slug: 'concepts/sources/microarch' },
            { slug: 'concepts/sources/gpu' },
            { slug: 'concepts/sources/thermal' },
            { slug: 'concepts/sources/signal' },
            { slug: 'concepts/sources/sensor' },
            { slug: 'concepts/sources/quantum' },
            { slug: 'concepts/conditioning' },
            { slug: 'concepts/telemetry' },
            { slug: 'concepts/architecture' },
          ],
        },
        {
          label: 'Guides',
          items: [
            { slug: 'guides/sdk-integration' },
            { slug: 'guides/troubleshooting' },
            { slug: 'guides/ai-search' },
          ],
        },
      ],
    }),
  ],
});
