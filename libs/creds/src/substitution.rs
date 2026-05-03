//! `{{secret:NAME}}` substitution + reverse rebuild.
//!
//! Pattern: stakpak_arch.md §27 (`{{secret:name}}` reference shape;
//! resolution at the tool-execution boundary).
//! Constitution: Article V (LLM never sees raw values; only references
//! travel through prompts), Article IV (malformed references and
//! unknown references both fail loudly).
//!
//! Stage 1 single-pass scan with strict reference shape:
//! `\{\{secret:[A-Za-z0-9_-]+\}\}`. Nested references rejected (would
//! require recursion + risks infinite loop).

use crate::broker::CredentialBroker;
use crate::errors::CredsError;
use std::collections::BTreeMap;

const PREFIX: &str = "{{secret:";
const SUFFIX: &str = "}}";

/// Reverse-rebuild map: `(substituted_value → reference_string)`.
/// Stored so a downstream consumer can rebuild the original
/// `{{secret:NAME}}` form for audit purposes (the audit log MUST
/// record the reference, not the value).
#[derive(Debug, Clone, Default)]
pub struct SubstitutionMap {
    entries: BTreeMap<String, String>,
}

impl SubstitutionMap {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Reverse-rebuild — replace every substituted value with its
    /// `{{secret:NAME}}` reference. Used by the audit emission path
    /// to ensure cred values never land in audit entries.
    ///
    /// Implementation note (security-auditor finding C2): we iterate
    /// values **longest-first** so a shorter value can't shadow a
    /// longer one that's a strict prefix. Without this, a fixture
    /// like `with_secret("a", "v")` could over-replace a coincidental
    /// `"v"` substring elsewhere in the payload. Stage 5+ may switch
    /// to span-based reconstruction (track original cursor offsets
    /// through `substitute()` rather than text-replace post-hoc) for
    /// stricter byte-exact correctness with arbitrary STS tokens.
    pub fn rebuild(&self, substituted: &str) -> String {
        let mut entries: Vec<(&String, &String)> = self.entries.iter().collect();
        entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        let mut out = substituted.to_string();
        for (value, reference) in entries {
            out = out.replace(value, reference);
        }
        out
    }

    /// Number of substitutions performed.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Replace every `{{secret:NAME}}` with the broker's resolved value.
/// Returns the substituted text + a `SubstitutionMap` for reverse
/// rebuild.
///
/// Article IV: unknown reference → loud `Err(UnknownReference)`;
/// malformed → `Err(MalformedReference)`.
///
/// Article XIII rule 3 (no string slice): we use `str::get(range) ->
/// Option<&str>` for every range access; bad boundaries fall back to
/// the empty string (which terminates the loop cleanly) instead of
/// panicking.
pub async fn substitute(
    payload: &str,
    broker: &dyn CredentialBroker,
) -> Result<(String, SubstitutionMap), CredsError> {
    let mut out = String::with_capacity(payload.len());
    let mut map = SubstitutionMap::new();
    let mut cursor = 0usize;

    loop {
        let remaining = payload.get(cursor..).unwrap_or("");
        let start_rel = match remaining.find(PREFIX) {
            Some(idx) => idx,
            None => {
                out.push_str(remaining);
                break;
            }
        };
        let start = cursor + start_rel;
        out.push_str(payload.get(cursor..start).unwrap_or(""));

        let body_start = start + PREFIX.len();
        let after_prefix = payload.get(body_start..).unwrap_or("");
        let end_rel = after_prefix.find(SUFFIX).ok_or_else(|| {
            CredsError::MalformedReference(payload.get(start..).unwrap_or("").to_string())
        })?;
        let body_end = body_start + end_rel;
        let name = payload.get(body_start..body_end).unwrap_or("");

        // Strict shape: NAME ∈ [A-Za-z0-9_-]+. Reject nested-reference
        // attempts and whitespace.
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(CredsError::MalformedReference(format!(
                "{{{{secret:{name}}}}}"
            )));
        }

        let credential = broker.resolve(name).await?;
        let value = credential.expose().to_string();
        let reference = format!("{{{{secret:{name}}}}}");
        map.entries.insert(value.clone(), reference);
        out.push_str(&value);

        cursor = body_end + SUFFIX.len();
    }

    Ok((out, map))
}
