# Alpha.4 two-hour endurance evidence

Run: https://github.com/LioRael/livequery/actions/runs/40248418492

Behavior source: `932841ac9fda5848f6fcab44bbf4ec2d37320d1d`. All four jobs passed on 2026-10-26. Subsequent documentation commits do not represent new tested binaries. Raw reports and toolchain records are retained alongside this summary.

| Platform / engine | Seconds | Requests | Observations | Busy observations / publications | Samples | RSS range bytes | FD range | Thread range |
|---|---:|---:|---:|---|---:|---:|---:|---:|
| darwin-async | 7200.027 | 258901900 | 948858 | 1358 / 81 | 1435 | 114688 | 4 | 0 |
| darwin-sync | 7200.048 | 216321899 | 791425 | 1360 / 71 | 1432 | 81920 | 4 | 0 |
| linux-async | 7200.002 | 236365426 | 1410825 | 35 / 4 | 1438 | 98304 | 4 | 0 |
| linux-sync | 7200.007 | 100041022 | 1400914 | 90 / 1 | 1437 | 77824 | 4 | 0 |

All business errors were zero. Diagnostic errors were exclusively Busy, each below the 1% observation/publication budget. All reports were independently rerun through `scripts/check-load.py --min-seconds 7200 --engine ENGINE`: continuous traffic, sampling coverage/gaps, duration and warmed resource ranges passed. Binary SHA-256 values are recorded in each raw JSON; both engines share the same binary on each platform. Toolchain: Rust 1.99.1, Linux x86_64 and macOS aarch64.

Scope: one-row loopback service fixture, four clients, four observers, requested 10 ms publication/poll intervals. This is eight aggregate worker-hours, not an eight-hour single run, production SLO, representative real-project validation or security certification. Tail latency covers only the last 100,000 requests. The alpha.3 run was canceled and provides no endurance pass evidence.

## Evidence file SHA-256

- `darwin-async/endurance.json`: `def14bad76933ecb8b113f71b14006e93f3ae1c4135ff4b13e1a1799c55515fe`
- `darwin-async/toolchain.txt`: `61d471bc8cf0eaa9ddb03db85c852dad15a430de049235d485c9f70bb3095909`
- `darwin-sync/endurance.json`: `545e4281ad5b672ca7c2c4471127d4e63f966bc0c07d997d9c557405dd93238e`
- `darwin-sync/toolchain.txt`: `61d471bc8cf0eaa9ddb03db85c852dad15a430de049235d485c9f70bb3095909`
- `linux-async/endurance.json`: `c180c0e5d7ce4f3bb15f5e85b58013e0953c7a994c7dc6c95a8e3b781761b1bc`
- `linux-async/toolchain.txt`: `a6cbeefa6b152477dda4483eb0d0183523ab13cd131366019a08bb4249dd8f23`
- `linux-sync/endurance.json`: `b3e685c7426d02ad61bbf2ae9f7c0e860655da93478076f7b0eae1d23ff7e101`
- `linux-sync/toolchain.txt`: `a6cbeefa6b152477dda4483eb0d0183523ab13cd131366019a08bb4249dd8f23`
