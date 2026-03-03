# Documentation IA Audit

Date: 2026-03-03

This audit evaluates how easy it is for users to find answers in openentropy docs
across CLI, Rust SDK, Python SDK, and validation workflows.

## Scope

- Repository docs: `README.md`, `docs/*.md`, `AGENTS.md`
- Docs site content: `website/src/content/docs/**`
- Docs site navigation: `website/astro.config.mjs`
- External benchmark set: Git, Docker, kubectl, uv, pnpm, GitHub CLI, Tokio,
  clap, reqwest, pydantic, FastAPI, requests, poetry

## External Patterns Observed

High-consistency patterns across successful CLI/SDK docs:

1. Clear split of onboarding vs task guides vs API/reference pages
2. Navigation optimized for user intent (role/task), not just internal module names
3. Dedicated reference entry points for each surface (CLI, SDKs, API)
4. Prominent cross-links on overview pages to reduce dead-end reading
5. Single canonical doc surface to reduce stale duplicates

## Current Findability Map

This map scores common user intents on a 1-5 scale:

- 5 = can find in one hop from home or role page
- 3 = can find with sidebar/search but not obvious
- 1 = likely missed without deep scan or prior repo knowledge

| User intent | Primary audience | Current path | Score | Notes |
|---|---|---|---:|---|
| Install and get first bytes | all | `getting-started/index.mdx` -> quickstart | 5 | strong entry path |
| Find every CLI command/flag | CLI | `cli/index.md` -> `cli/reference.md` | 5 | clear |
| Find Python API by function | Python | `python-sdk/reference.md` | 4 | good, long page |
| Find Rust API by type/function | Rust | `rust-sdk/api.md` | 4 | good, long page |
| Pick docs by role | all | `getting-started/choose-your-path.mdx` | 5 | strong |
| Security validation workflow | security | `guides/security-validation.md` | 4 | discoverable via guides + CLI |
| Research workflow | research | `guides/research-methodology.md` | 4 | discoverable |
| Understand analysis verdicts | security/research | `concepts/analysis.md` -> subpages | 4 | split is good |
| Find full integration patterns | integrators | mostly `docs/INTEGRATIONS.md` | 2 | website coverage partial |
| Find complete 63-source catalog in one place | research | split hub + category pages | 3 | comprehensive but fragmented |

## Friction Points

1. Dual doc surfaces (`/docs` and website docs) create source-of-truth ambiguity.
2. README doc table points to `/docs/*.md`, while docs site is the navigable user
   surface.
3. Integration content is richer in `docs/INTEGRATIONS.md` than in the website guide.
4. Some pages end with `Related`, others with `Next Steps`, which weakens scan
   predictability.
5. Long reference pages have strong completeness but moderate scanning friction.

## IA Recommendations (Applied In This Iteration)

1. Keep website docs as the primary user navigation surface.
2. Add a first-class website Integrations guide to close a major discoverability gap.
3. Publish explicit docs standards and canonical-surface policy in-repo.
4. Update README doc links to canonical website routes for direct discoverability.
5. Add docs mirror policy under `docs/` to reduce drift risk when both surfaces exist.

## Success Metrics

Track these in future iterations:

- Task-to-answer hops (target: <= 2 clicks for top 10 intents)
- Orphan-page count in website nav (target: 0 for user-facing guides)
- Legacy-only user-facing docs count under `docs/` (target: 0)
- Cross-surface drift incidents per release (target: 0)
- % of top-level docs pages with consistent endcap section (`Next Steps`) (target: 100%)
