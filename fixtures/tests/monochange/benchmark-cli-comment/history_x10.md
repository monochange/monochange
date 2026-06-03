| Command                            |     Mean [ms] | Min [ms] | Max [ms] |    Relative |
| :--------------------------------- | ------------: | -------: | -------: | ----------: |
| `main · monochange step validate`               |  480.0 ± 12.0 |    462.0 |    495.0 |        1.00 |
| `pr · monochange step validate`                 |  502.0 ± 15.0 |    481.0 |    522.0 | 1.05 ± 0.05 |
| `main · monochange step discover --format json` |  390.0 ± 14.0 |    371.0 |    409.0 |        1.00 |
| `pr · monochange step discover --format json`   |  372.0 ± 11.0 |    358.0 |    388.0 | 0.95 ± 0.04 |
| `main · monochange run release --dry-run`      |  980.0 ± 28.0 |    942.0 |   1018.0 |        1.00 |
| `pr · monochange run release --dry-run`        | 1045.0 ± 35.0 |    998.0 |   1091.0 | 1.07 ± 0.05 |
| `main · monochange run release`                | 1185.0 ± 41.0 |   1132.0 |   1243.0 |        1.00 |
| `pr · monochange run release`                  | 1264.0 ± 46.0 |   1208.0 |   1327.0 | 1.07 ± 0.05 |
