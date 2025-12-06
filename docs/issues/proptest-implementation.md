# Add Property-Based Tests (Proptests)

## Summary

Add property-based testing using the `proptest` crate to verify protocol invariants and encode/decode correctness for core modules.

## Background

The project already has:
- Fuzz tests for subtle-tls (TLS parsing)
- Criterion benchmarks for CPU-bound operations
- E2E tests using Playwright

Property-based tests complement these by systematically testing invariants with randomized inputs, catching edge cases that manual tests miss.

## Modules to Test

### 1. turbo.rs - TurboFrame Protocol

Priority: High (variable-length header encoding is error-prone)

**Properties:**

- [ ] **Roundtrip**: Any data encodes then decodes back correctly with preserved padding flag
- [ ] **Header size boundaries**: Correct header size at thresholds (0x3F, 0x40, 0x1FFF, 0x2000, 0xFFFFF)
- [ ] **Partial decode**: Any proper prefix of encoded frame returns `Ok(None)`
- [ ] **Oversized rejection**: Headers claiming length > MAX_FRAME_SIZE return error

```rust
proptest! {
    #[test]
    fn turbo_frame_roundtrips(
        data in proptest::collection::vec(any::<u8>(), 0..=0xFFFFF),
        is_padding in any::<bool>(),
    ) {
        let frame = if is_padding {
            TurboFrame::padding(data.clone())
        } else {
            TurboFrame::new(data.clone())
        };
        let encoded = frame.encode();
        let (decoded, consumed) = TurboFrame::decode(&encoded).unwrap().unwrap();
        
        prop_assert_eq!(decoded.data, data);
        prop_assert_eq!(decoded.is_padding, is_padding);
        prop_assert_eq!(consumed, encoded.len());
    }
}
```

### 2. smux.rs - SMUX Multiplexing Protocol

Priority: High (little-endian encoding, multiple message types)

**Properties:**

- [ ] **Segment roundtrip**: All command types encode/decode correctly
- [ ] **SmuxUpdate roundtrip**: consumed/window u32 values preserved across full range
- [ ] **Invalid rejection**: Invalid versions (!=2) and commands (>4) return errors
- [ ] **Partial decode**: Incomplete segments return `Ok(None)`
- [ ] **Short UPD rejection**: UPD payloads < 8 bytes return error

```rust
proptest! {
    #[test]
    fn smux_update_roundtrips(consumed in any::<u32>(), window in any::<u32>()) {
        let seg = SmuxSegment::upd(3, consumed, window);
        let upd = SmuxUpdate::decode(&seg.data).unwrap();
        
        prop_assert_eq!(upd.consumed, consumed);
        prop_assert_eq!(upd.window, window);
    }
}
```

### 3. relay.rs - Relay Selection

Priority: Medium (complex filtering logic)

**Properties:**

- [ ] **Flag invariants**: Selected relays have all required flags, none of excluded flags
- [ ] **Fingerprint exclusion**: Excluded fingerprints never selected
- [ ] **Bandwidth filter**: Selected relays meet minimum bandwidth
- [ ] **Max selection**: Result size <= max_selection
- [ ] **Ordering**: Results sorted by consensus_weight descending
- [ ] **Consistency**: `select_relay` returns element from `select_relays`
- [ ] **Error consistency**: Empty candidates produce error from both methods

```rust
proptest! {
    #[test]
    fn relay_selection_respects_invariants(
        relays in proptest::collection::vec(relay_strategy(), 0..=16),
        criteria in criteria_strategy(),
    ) {
        let mgr = RelayManager::new(relays);
        let result = mgr.select_relays(&criteria);
        
        if let Ok(selected) = result {
            prop_assert!(selected.len() <= criteria.max_selection);
            for r in &selected {
                for f in &criteria.need_flags {
                    prop_assert!(r.flags.contains(f));
                }
                // ... more invariants
            }
        }
    }
}
```

## Implementation Plan

### Step 1: Add Dependencies

```toml
# webtor/Cargo.toml
[dev-dependencies]
proptest = "1"
```

### Step 2: Add Tests (inline with existing tests)

Structure each module as:
```rust
#[cfg(test)]
mod tests {
    mod unit { /* existing tests */ }
    
    #[cfg(not(target_arch = "wasm32"))]
    mod proptests {
        use super::super::*;
        use proptest::prelude::*;
        // property tests here
    }
}
```

### Step 3: Configure Test Cases

- Encode/decode tests: 256 cases (default)
- Relay selection: 64-128 cases (heavier data structures)

Use `ProptestConfig` to tune per-test if needed.

## Test Commands

```bash
# Run all tests including proptests
cargo test --package webtor

# Run only proptests (by naming convention)
cargo test --package webtor proptest
```

## CI Integration

A GitHub Action runs proptests on every PR (see `.github/workflows/proptest.yml`).

## Acceptance Criteria

- [ ] All properties listed above have passing tests
- [ ] Tests complete in < 30 seconds locally
- [ ] CI workflow runs on every PR
- [ ] No flaky tests (use fixed seeds if needed)

## References

- [proptest book](https://proptest-rs.github.io/proptest/intro.html)
- Oracle consultation on implementation approach (thread T-f7db4f46)
