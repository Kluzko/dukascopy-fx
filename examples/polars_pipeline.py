"""Example polars pipeline using fx_fetcher output."""

import polars as pl


def main() -> None:
    columns = [
        "symbol",
        "base",
        "quote",
        "timestamp",
        "rate",
        "bid",
        "ask",
        "bid_volume",
        "ask_volume",
    ]

    df = pl.read_csv("data/fx.csv", has_header=False, new_columns=columns)
    print(df.head())


if __name__ == "__main__":
    main()
