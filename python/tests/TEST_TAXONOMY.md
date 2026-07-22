# Test Taxonomy

## Tier 0 — Unit Implementation Tests
Tests that verify internal implementation details. Not used as compatibility evidence.
Examples: test_milestone_c_functional.py, test_milestone_c_properties.py

## Tier 1 — Candidate Contract Tests
Tests that verify the candidate (eggress) matches its own API contract.
Examples: test_asyncio_semantic.py, test_protocol_behavioral.py, test_cipher_truth.py

## Tier 2 — Paired Oracle Differential Tests
Tests that run the same probe against both oracle and candidate and compare results.
Gated on EGRESS_REQUIRE_PPROXY_DIFFERENTIAL=1.
Examples: test_pproxy_differential.py, test_pproxy_oracle.py

## Tier 3 — External Interoperability Tests
Tests that verify actual network interoperability with real implementations.
Gated on EGRESS_REQUIRE_EXTERNAL_INTEROP=1.
Examples: interoperability_shadowsocks.rs (Rust)

## Tier 4 — Platform Tests
Tests that verify platform-specific behavior.
Examples: transparent proxy tests, PF tests

## Tier 5 — Release Certification Tests
Tests that must pass before a release is certified.
Examples: strict manifest validation, report freshness
