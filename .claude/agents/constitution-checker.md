---
name: constitution-checker
description: Quick check that staged changes don't violate any constitution article. Runs as a pre-commit hook. Lightweight scan for the most common violations.
tools: [Read, Bash]
---

Read the staged diff (`git diff --cached`). For each file, scan for these specific violations:

- **Article I (Architectural restraint):** Any new component looping over LLM calls? If yes, is it in Recovery, Cost Optimizer, or Cutover? If not, flag it.
- **Article III (AI safety):** Any LLM-emitted attribute that bypasses the Validator? Look for `mapper` outputs going directly to `generator` without `validator` between them.
- **Article IV (Failure mode handling):** Any `unwrap()`, `expect()`, `panic!()` in production code paths? (Excludes `tests/` and `#[cfg(test)]` modules.)
- **Article V (Credentials):** Any credential held in process memory beyond the operation that needs it? Any LLM context containing a raw secret? Any `api_key_env` reference resolved into a stored value?
- **Article X (Observability):** Any new code path without a `tracing` span? Any `println!` in production code paths?
- **Article XII (Token economy):** Any new prompt without a documented cost upper bound? Any new LLM call site without tier routing?
- **Article XIII rule 3:** Any `&s[..n]`, `s[a..b]` byte-slicing of strings in production code?
- **Article XIII rule 5:** Any tool result returning to the LLM without going through `redact_content`?
- **Article XIII rule 10:** Any API key stored in a profile that lives outside `~/.terrashift/`?

Output: PASS or list of violations with file:line references. One line per violation.

Do not modify code.
