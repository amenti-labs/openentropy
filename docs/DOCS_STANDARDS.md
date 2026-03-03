# Documentation Standards

Date: 2026-03-03

This document defines organizational standards for openentropy docs to keep CLI,
Rust SDK, Python SDK, server, and conceptual docs easy to find and hard to drift.

## Canonical Surfaces

- User-facing navigation and IA: `website/src/content/docs/**`
- Docs sidebar/source ordering: `website/astro.config.mjs`
- Legacy mirror docs: `docs/*.md` (non-canonical, compatibility surface)

When behavior changes, update website docs first. Mirror docs are optional and
must never be the first or only place where user-visible behavior is updated.

## Legacy Mirror Policy

- `docs/*.md` is a compatibility mirror, not the source of truth.
- Do not add net-new canonical content only in `docs/*.md`.
- If a mirror page is maintained, keep command/API examples consistent with the
  website canonical page for that topic.
- README and cross-doc links should prefer canonical website routes.

## Required Page Types

Each major surface (CLI, Rust SDK, Python SDK) should have:

1. Overview page (`index`) for orientation and first steps
2. Quick reference page for common workflows
3. Full reference page for complete API/command coverage

Conceptual domains should use one hub + focused deep-dive pages.

## Navigation Rules

- Sidebar entries must map to real files.
- New user-facing guides must be added to the sidebar in the same change.
- Cross-links should use canonical website paths:
  `/openentropy/<section>/<page>/`

## Content Format Rules

- Website docs must include frontmatter: `title`, `description`
- Use `## Next Steps` at the end of overview/guides pages
- Use language-tagged code blocks: `bash`, `rust`, `python`
- Use absolute website-internal links for docs-site pages

## Drift Prevention Rules

- Do not introduce commands/flags/examples not implemented in code.
- Prefer one canonical version of user-facing content (website docs).
- Update docs links in `README.md` when docs IA changes.
- Keep AGENTS cross-surface alignment requirements in force.

## Verification Checklist

For docs IA/content updates:

1. Build docs site (`website`: `npm run build`)
2. Validate new/changed sidebar slugs resolve to existing files
3. Spot-check updated links and route paths
4. Confirm changed examples align with current CLI/API names
