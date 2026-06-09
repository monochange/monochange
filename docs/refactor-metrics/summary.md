# monochange CLI extraction metrics

Baseline: `origin/main` at `bf2bf3301`. After: this branch after extracting `monochange_cli`.

## Lines of code

| Area                        | Before |  After |   Delta |
| --------------------------- | -----: | -----: | ------: |
| `crates/monochange/src`     | 68,535 |      9 | -68,526 |
| `crates/monochange_cli/src` |      0 | 68,534 | +68,534 |

The published `monochange` package is now a tiny facade/binary shim. The CLI implementation moved into `monochange_cli` with no behavioral rewrite in this step.

## Binary size

Built with `cargo build --profile dist -p monochange`.

| Build  |      Bytes | Approx |
| ------ | ---------: | -----: |
| Before | 17,299,024 |  17 MB |
| After  | 17,001,152 |  17 MB |
| Delta  |   -297,872 | -1.72% |

## Runtime smoke timings

Each command ran 7 times from the repository root, discarding stdout/stderr. Values below are medians in milliseconds; full samples are in `performance.json`.

| Command                                  | Before median | After median |    Delta |
| ---------------------------------------- | ------------: | -----------: | -------: |
| `monochange --help`                      |       6.12 ms |      7.49 ms | +1.37 ms |
| `monochange --snapshot`                  |     123.92 ms |    119.44 ms | -4.48 ms |
| `monochange step validate --format json` |     113.27 ms |    115.75 ms | +2.48 ms |

The extraction is performance-neutral for the measured CLI paths. Binary size decreased slightly despite the additional crate boundary.
