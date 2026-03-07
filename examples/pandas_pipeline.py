"""Example pandas pipeline using fx_fetcher output."""

import pandas as pd


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

    df = pd.read_csv("data/fx.csv", names=columns)
    df["timestamp"] = pd.to_datetime(df["timestamp"], utc=True)
    print(df.head())


if __name__ == "__main__":
    main()
