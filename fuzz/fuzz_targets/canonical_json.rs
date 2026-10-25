//! Fuzz the evidence boundary: arbitrary bytes parsed as JSON and
//! canonicalized. Canonicalization must never panic, and its output must be a
//! fixed point — re-parsing canonical bytes and canonicalizing again yields
//! the same bytes. A violation would mean one logical value has two hashes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use treasury_evidence::canonical_bytes;

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
        return;
    };
    let Ok(first) = canonical_bytes(&value) else {
        return;
    };
    let reparsed: serde_json::Value =
        serde_json::from_slice(&first).expect("canonical output is valid JSON");
    let second = canonical_bytes(&reparsed).expect("canonical output re-canonicalizes");
    assert_eq!(first, second, "canonicalization is not idempotent");
});
