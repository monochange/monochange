| Command                            |    Mean [ms] | Min [ms] | Max [ms] |    Relative |
| :--------------------------------- | -----------: | -------: | -------: | ----------: |
| `main · monochange step validate`               |  120.0 ± 6.0 |    113.0 |    129.0 |        1.00 |
| `pr · monochange step validate`                 |  126.0 ± 4.0 |    121.0 |    131.0 | 1.05 ± 0.06 |
| `main · monochange step discover --format json` |   88.0 ± 3.0 |     84.0 |     92.0 |        1.00 |
| `pr · monochange step discover --format json`   |   85.0 ± 2.0 |     82.0 |     88.0 | 0.97 ± 0.04 |
| `main · monochange run release --dry-run`      |  240.0 ± 8.0 |    229.0 |    251.0 |        1.00 |
| `pr · monochange run release --dry-run`        | 255.0 ± 10.0 |    242.0 |    269.0 | 1.06 ± 0.06 |
| `main · monochange run release`                |  315.0 ± 9.0 |    302.0 |    329.0 |        1.00 |
| `pr · monochange run release`                  | 334.0 ± 11.0 |    319.0 |    348.0 | 1.06 ± 0.05 |
