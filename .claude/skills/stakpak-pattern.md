---
name: stakpak-pattern
description: Loaded when porting a specific Stakpak pattern to Terrashift. Use when implementing anything that mirrors a Stakpak subsystem.
---

When porting a Stakpak pattern to Terrashift:

1. **Read `stakpak_arch.md` first.** The architecture doc at `refs/stakpak_arch.md` is the canonical citation. Find the section that documents the pattern.
2. **Descend into source only when needed.** If the doc references a specific file:line, open `refs/stakpak/<path>` and verify. When doc and code disagree, follow the code; document the discrepancy.
3. **Identify what changes for Terrashift:**
   - Different domain (Stakpak = general DevOps; Terrashift = cross-cloud Terraform migration).
   - Different user (Stakpak = engineers debugging infra; Terrashift = teams running migrations).
   - Different scope (Stakpak ships many features; Terrashift Stage 1 is narrow).
4. **Adapt, don't transliterate.** Lines-of-code count doesn't have to match. The pattern is what matters; the implementation should fit Terrashift's domain.
5. **Cite the source files in code comments** — at the top of any file that adopted a Stakpak pattern:
   ```rust
   //! Pattern: stakpak_arch.md section N
   //! Source: refs/stakpak/libs/<path>
   ```
6. **Note the deviation in PR description** with reasoning. Reviewers check that deviations are intentional.
7. **If the deviation is significant**, propose adding it to `ATTRIBUTIONS.md` under "Adopted with adaptation."

Anti-patterns to avoid (Article XIII):

- **Rule 1:** Don't bypass message conversion pipeline (`ContextReducer::reduce`). Anthropic 400s on dangling `tool_use` blocks.
- **Rule 4:** Don't conflate `ChatMessage` (storage type) and `LLMMessage` (runtime type). Mixing them creates conversion mistakes that compile but fail at runtime. Boundary lives in `libs/shared`.
