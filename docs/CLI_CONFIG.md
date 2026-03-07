# fx_fetcher CLI Config

`fx_fetcher` can load defaults from a TOML file via:

```bash
fx_fetcher --config config/fx_fetcher.example.toml <command> [...flags]
```

Flag precedence:

1. Explicit CLI flags (highest)
2. Config file values (`--config`)
3. Built-in defaults (lowest)

## Schema

```toml
[global]
universe = "config/universe.json"
checkpoint = ".state/checkpoints.json"
concurrency = 8
json = false

[backfill]
symbols = ["EURUSD", "GBPUSD"]
period = "30d"
interval = "1h"
out = "data/fx.csv"
no_output = false
concurrency = 8

[update]
symbols = ["EURUSD", "GBPUSD"]
lookback = "7d"
interval = "1h"
out = "data/fx.csv"
no_output = false
concurrency = 8

[sync_universe]
source = "https://www.dukascopy-node.app"
dry_run = false
activate_new = false

[export]
input = "data/fx.csv"
out = "data/fx.parquet"
has_headers = false
```

Reference file: `config/fx_fetcher.example.toml`.
