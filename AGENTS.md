# Repository agent rules

This repository follows a clean-room implementation policy.

## Prohibited

1. Do not decompile, disassemble, extract, or inspect Adobe After Effects, Photoshop, Premiere, or other Adobe binary content, including string tables and resource sections.
2. Do not copy Adobe icons, pointers, fonts, audio, presets (`.ffx`), templates, screenshots, or text from product documentation into the product.
3. Do not use Adobe trademarks or product logos in the Motion Studio product name, logo, icons, UI, package identifiers, or release artwork.
4. Do not add third-party assets without recording the author, source, revision, license, and modification scope in `ATTRIBUTION.md`.

## Permitted references

Public Adobe user guides, public scripting/API references, public shortcut references, standards, papers, and community discussions may be used to understand documented behavior. The implementation, assets, and product language must remain independently authored.

Before adding a dependency, asset, compatibility name, or copied-looking UI detail, check `PROVENANCE.md`, `ATTRIBUTION.md`, and `NOTICE`. Flag any violation directly in the pull request.

## Visual style

`DESIGN.md` is the single source of truth for UI presentation. `desktop/ui/styles/dark.json` carries the token values; `DESIGN.md` carries the rules, the remediation baseline, and the acceptance checklist. When they disagree, `dark.json` wins on values and `DESIGN.md` wins on rules.

Never write a literal color or dimension in panel code — reference a semantic token. `tools/ci_desktop.py` fails the build on `palette::` or `default_stylesheet(` in `desktop/app/src/panels`. `DESIGN.md` §2 forbids inventing `accent-strong`, `playhead`, or `layer-label-*` style names that have no existing token. Any token change must be written to `dark.json` first, never in paint code.

Property trees and effect lists have a hard row contract, `DESIGN.md` §5.2. One property per row, all controls inline, every dimension read from a token. Never render a switch or an enum as a `ui::button`, never cycle enum values on click, never open a value field's background unless it is being edited, and never add a row to make room for a control. Do not shrink existing hit tolerances to gain density. Those are the most common regressions in `desktop/app/src/panels`.

`DESIGN.md` §0.7 is the remediation baseline: every row is a known existing violation with a file and line. Check it before editing a panel, and report each row as fixed / retained with reason / pending verification. `DESIGN.md` §12 is the acceptance checklist; compilation success is not acceptance.
