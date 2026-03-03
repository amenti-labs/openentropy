---
title: 'AI Search Setup'
description: 'Enable Algolia DocSearch + Ask AI for the docs site'
---

The docs site uses Starlight's default Pagefind search out of the box.

To enable AI search, configure Algolia DocSearch (and optionally Ask AI).

## 1) Install dependencies

The site already includes the plugin dependency:

```bash
npm install @astrojs/starlight-docsearch
```

## 2) Configure environment variables

Set these variables before running `npm run dev` or `npm run build`:

```bash
export ALGOLIA_APP_ID="your_app_id"
export ALGOLIA_SEARCH_API_KEY="your_search_api_key"
export ALGOLIA_INDEX_NAME="your_index_name"
```

Optional (enables Ask AI UI in DocSearch):

```bash
export ALGOLIA_ASK_AI_ASSISTANT_ID="your_assistant_id"
```

## 3) Start the docs site

```bash
npm run dev
```

When all required Algolia variables are present, Starlight switches from Pagefind to DocSearch.

If variables are missing, the site automatically falls back to Pagefind.

## 4) Verify

- Open the search modal (`Cmd+K` / `Ctrl+K`).
- Confirm results are coming from Algolia index content.
- If `ALGOLIA_ASK_AI_ASSISTANT_ID` is set, confirm Ask AI appears in the search UI.

## Notes

- Required for DocSearch: `ALGOLIA_APP_ID`, `ALGOLIA_SEARCH_API_KEY`, `ALGOLIA_INDEX_NAME`
- Optional for Ask AI: `ALGOLIA_ASK_AI_ASSISTANT_ID`
- No secrets are committed in the repository; use environment variables locally and in CI.
