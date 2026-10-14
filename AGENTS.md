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
