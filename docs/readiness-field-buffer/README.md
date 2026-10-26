# Bounded field encoding allocation improvement

Baseline source: 028499390432a34f6baa37b96f5440e56cdb41fe. Same existing `cargo bench -p livequery-benchmarks --bench jobs`, native Rust 1.99.0, release strip disabled locally; before then after, not a randomized or CI comparison. Raw logs and JSON retained. No new endurance workflow or acceptance threshold.

`value_with_budget` previously allocated a temporary Vec for every serialized field, even a single number. The private capped writer now supports a 128-byte inline field buffer, spilling once into a Vec for larger encoded values without serializing twice. Public `json_bytes` uses the same writer with zero inline capacity and still returns its owned Vec. Serde encoding/parsing, depth/numeric behavior, remaining-budget accounting, snapshot validation and 100ms wall deadline remain unchanged.

For 5000-row existing fixtures, 16-byte strings reduced capture allocations from 70,153 to 35,153 and mean capture time from 5419.700us to 4614.881us. With 128-byte strings, allocations fell from 70,154 to 50,154 and mean time from 6229.856us to 6059.110us. The baseline contention cases had no errors; the candidate had one permitted Busy publication in a 1000-row case and no Deadline. These short local benchmark results establish fewer temporary allocations, not elimination of CI scheduling stalls or successful final acceptance. The existing benchmark fixture differs from the 5000-row/96-byte endurance workload.

New tests cover encoded sizes below/at/above the inline boundary, escaping/Unicode, nested values, every byte-budget boundary and remaining budget on failure, and single invocation of a custom serializer on spill. Core tests with arbitrary_precision/raw_value passed.

The first workspace test run failed in the CLI watch-state fixture: accepted sockets were read as blocking without explicitly clearing inherited nonblocking mode, causing an immediate WouldBlock on macOS. The fixture now explicitly sets blocking mode before installing its existing unchanged timeouts. The failure log is retained; this fixture issue is separate from the capture Deadline investigation.

The existing diagnostic archive builder must use its checked-out HEAD rather than hard-coded 12fa024, otherwise a new implementation would silently test the old library. Build artifact/provenance and explicit diagnostic patch remain required. Only one candidate run should be dispatched after native validation; no repeated green-seeking trials.
