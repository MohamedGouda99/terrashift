---
description: Run the token-cost analysis from P-19. Article XII enforcement.
---

Run a token cost audit per P-19 in `terrashift_prompts.md`.

Pull the latest `eval-runner` output (or invoke the sub-agent if no fresh data). Apply Article XII rules in priority:

1. **Cache hit rate per prompt.** Article XII rule 2 (cache-first). Per Article XIII rule 2 — verify `trimmed_up_to_message_index` is monotonic; non-monotonic boundaries kill Anthropic prompt-cache hits.
2. **Tier routing decisions.** Article XII rule 3 (tiered routing). For each LLM call site, confirm the right tier is used. Mapper/Planner = `eco`, Recovery/Cost Optimizer = `smart`.
3. **Context window sizes.** Per terrashift_plan.md §6.8 — graph-aware retrieval should keep each Mapper call under ~3K tokens, not 8-15K.
4. **Pure-code path bypassed?** Article I (deterministic-first). For each LLM call, ask: was there a deterministic path that should have been tried first?

Output: top-5 most expensive prompts, with optimization suggestions. Cite which Article XII rule each suggestion enforces.
