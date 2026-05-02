//! Chain verification — reads exported entries, verifies hashes + signatures.
//!
//! Pattern: terrashift_plan.md §6.X (`verify_chain` invariant).
//! Constitution: Article V (auditors must be able to verify after the fact).

use crate::entry::AuditEntry;
use crate::errors::AuditError;
use crate::signer::verify_signature;
use crate::store::compute_content_hash;

/// Verify a sequence of entries from `export()` against the run's verify key.
///
/// Walks oldest → newest, checking:
///   - prev_hash matches the previous entry's content_hash
///   - content_hash matches recomputed hash of the entry's data
///   - signature is valid against the verify key
pub fn verify_chain(entries: &[AuditEntry], verify_key_bytes: &[u8; 32]) -> Result<(), AuditError> {
    let mut expected_prev: [u8; 32] = [0u8; 32];

    for entry in entries {
        // 1. Chain link: this entry's prev_hash should match the previous
        //    entry's content_hash (or zeros for entry #0).
        if entry.prev_hash != expected_prev {
            return Err(AuditError::ChainBroken {
                entry_id: entry.id.to_string(),
                reason: format!(
                    "prev_hash mismatch — expected {:?} got {:?}",
                    expected_prev, entry.prev_hash
                ),
            });
        }

        // 2. Content hash: recompute and compare.
        let recomputed = compute_content_hash(entry)?;
        if recomputed != entry.content_hash {
            return Err(AuditError::ChainBroken {
                entry_id: entry.id.to_string(),
                reason: "content_hash does not match recomputed hash — entry was tampered"
                    .to_string(),
            });
        }

        // 3. Signature: must verify against the run's verify key.
        verify_signature(verify_key_bytes, &entry.content_hash, &entry.signature).map_err(|e| {
            match e {
                AuditError::SignatureInvalid { reason, .. } => AuditError::SignatureInvalid {
                    entry_id: entry.id.to_string(),
                    reason,
                },
                other => other,
            }
        })?;

        // 4. Advance for next iteration
        expected_prev = entry.content_hash;
    }

    Ok(())
}
