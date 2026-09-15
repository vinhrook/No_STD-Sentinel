// math/mod.rs — This file tells Rust "the `math` module contains sub-modules"
//
// When lib.rs says `pub mod math;`, Rust looks for EITHER:
//   1. src/math.rs  (a single file module)
//   2. src/math/mod.rs  (a directory module — this is what we're using)
//
// We use the directory form because `math` will contain MULTIPLE files:
//   - fixed_point.rs (Q16.16 arithmetic)
//   - filters.rs (signal processing — later)
//   - statistics.rs (RMS, kurtosis — later)

pub mod fixed_point;
