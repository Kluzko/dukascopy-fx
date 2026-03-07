use chrono::{DateTime, Duration, Utc};
use dukascopy_fx::advanced::{
    last_available_tick_time, resolve_instrument_config, ConfiguredClient, DukascopyClientBuilder,
};
use dukascopy_fx::storage::checkpoint::CheckpointStore;
#[cfg(feature = "sinks-parquet")]
use dukascopy_fx::storage::sink::ParquetSink;
use dukascopy_fx::storage::sink::{CsvSink, DataSink, NoopSink};
#[cfg(feature = "sinks-parquet")]
use dukascopy_fx::CurrencyPair;
use dukascopy_fx::{
    AssetClass, CurrencyExchange, DukascopyError, FileCheckpointStore, InstrumentCatalog,
    InstrumentDefinition, Ticker,
};
use quick_xml::events::Event;
#[cfg(feature = "sinks-parquet")]
use rust_decimal::Decimal;
use scraper::{Html, Selector};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::env;
use std::fs;
#[cfg(feature = "sinks-parquet")]
use std::str::FromStr;
use std::sync::Arc;
use tokio::task::JoinSet;

const DEFAULT_UNIVERSE_PATH: &str = "config/universe.json";
const DEFAULT_CHECKPOINT_PATH: &str = ".state/checkpoints.json";
const DEFAULT_CONCURRENCY: usize = 8;
const DEFAULT_DISCOVERY_SOURCE: &str = "https://www.dukascopy-node.app";
const CATEGORY_FETCH_CONCURRENCY: usize = 8;

struct BackfillFetchResult {
    ticker: Ticker,
    rows: Result<Vec<CurrencyExchange>, DukascopyError>,
}

struct UpdateJob {
    ticker: Ticker,
    checkpoint_key: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

struct UpdateFetchResult {
    ticker: Ticker,
    checkpoint_key: String,
    rows: Result<Vec<CurrencyExchange>, DukascopyError>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return Err("Missing command. See usage above.".into());
    }

    match args[0].as_str() {
        "--help" | "-h" => {
            print_usage();
        }
        "list-instruments" => {
            if has_flag(&args[1..], "--help") {
                println!("fx_fetcher list-instruments [--universe PATH]");
                return Ok(());
            }
            validate_flags(&args[1..], &["--universe"], &[])?;
            let universe_path = read_flag_value(&args[1..], "--universe")
                .unwrap_or_else(|| DEFAULT_UNIVERSE_PATH.to_string());
            list_instruments(&universe_path)?;
        }
        "backfill" => {
            run_backfill(&args[1..]).await?;
        }
        "update" => {
            run_update(&args[1..]).await?;
        }
        "sync-universe" => {
            run_sync_universe(&args[1..]).await?;
        }
        "export" => {
            run_export(&args[1..])?;
        }
        _ => {
            print_usage();
            return Err(format!("Unknown command '{}'", args[0]).into());
        }
    }

    Ok(())
}

fn print_usage() {
    println!("fx_fetcher - Dukascopy fetcher CLI");
    println!();
    println!("Usage:");
    println!("  fx_fetcher list-instruments [--universe PATH]");
    println!(
        "  fx_fetcher backfill [--universe PATH] [--symbols EURUSD,GBPUSD] [--period 30d] [--interval 1h] [--checkpoint PATH] [--out PATH.(csv|parquet) | --no-output] [--concurrency N]"
    );
    println!(
        "  fx_fetcher update [--universe PATH] [--symbols EURUSD,GBPUSD] [--lookback 7d] [--interval 1h] [--checkpoint PATH] [--out PATH.(csv|parquet) | --no-output] [--concurrency N]"
    );
    println!(
        "  fx_fetcher sync-universe [--universe PATH] [--source URL] [--dry-run] [--activate-new]"
    );
    println!("  fx_fetcher export --input PATH.csv --out PATH.parquet [--has-headers]");
    #[cfg(not(feature = "sinks-parquet"))]
    println!("  note: parquet export/output needs --features sinks-parquet");
}

fn print_backfill_usage() {
    println!(
        "fx_fetcher backfill [--universe PATH] [--symbols EURUSD,GBPUSD] [--period 30d|1mo|1y] [--interval 1h] [--checkpoint PATH] [--out PATH.(csv|parquet) | --no-output] [--concurrency N]"
    );
}

fn print_update_usage() {
    println!(
        "fx_fetcher update [--universe PATH] [--symbols EURUSD,GBPUSD] [--lookback 7d|1mo|1y] [--interval 1h] [--checkpoint PATH] [--out PATH.(csv|parquet) | --no-output] [--concurrency N]"
    );
}

fn print_sync_universe_usage() {
    println!(
        "fx_fetcher sync-universe [--universe PATH] [--source URL] [--dry-run] [--activate-new]"
    );
}

fn print_export_usage() {
    println!("fx_fetcher export --input PATH.csv --out PATH.parquet [--has-headers]");
}

fn validate_flags(
    args: &[String],
    flags_with_values: &[&str],
    flags_without_values: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut i = 0usize;
    while i < args.len() {
        let current = &args[i];
        if !current.starts_with("--") {
            return Err(format!("Unexpected positional argument '{}'", current).into());
        }

        if flags_with_values.contains(&current.as_str()) {
            let Some(value) = args.get(i + 1) else {
                return Err(format!("Missing value for option '{}'", current).into());
            };
            if value.starts_with("--") {
                return Err(format!("Missing value for option '{}'", current).into());
            }
            i += 2;
            continue;
        }

        if flags_without_values.contains(&current.as_str()) {
            i += 1;
            continue;
        }

        return Err(format!("Unknown option '{}'", current).into());
    }

    Ok(())
}

fn list_instruments(universe_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let catalog = InstrumentCatalog::from_file(universe_path)?;
    let instruments = catalog.active_instruments();

    println!(
        "Loaded {} active instruments from {}",
        instruments.len(),
        universe_path
    );
    println!(
        "{:<10} {:<8} {:<8} {:<10} {:<8} {:<10}",
        "Symbol", "Base", "Quote", "Class", "Decimals", "Divisor"
    );
    println!("{}", "-".repeat(64));
    for instrument in instruments {
        println!(
            "{:<10} {:<8} {:<8} {:<10} {:<8} {:<10.0}",
            instrument.symbol,
            instrument.base,
            instrument.quote,
            format!("{:?}", instrument.asset_class).to_lowercase(),
            instrument.decimal_places,
            instrument.price_divisor
        );
    }

    Ok(())
}

#[derive(Debug, Default)]
struct SyncUniverseStats {
    existing_count: usize,
    discovered_count: usize,
    present_count: usize,
    new_count: usize,
    reclassified_count: usize,
}

#[derive(Serialize)]
struct PersistedCatalog {
    instruments: Vec<InstrumentDefinition>,
    code_aliases: BTreeMap<String, String>,
}

async fn run_sync_universe(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if has_flag(args, "--help") {
        print_sync_universe_usage();
        return Ok(());
    }
    validate_flags(
        args,
        &["--universe", "--source"],
        &["--dry-run", "--activate-new"],
    )?;

    let universe_path =
        read_flag_value(args, "--universe").unwrap_or_else(|| DEFAULT_UNIVERSE_PATH.to_string());
    let source =
        read_flag_value(args, "--source").unwrap_or_else(|| DEFAULT_DISCOVERY_SOURCE.to_string());
    let dry_run = has_flag(args, "--dry-run");
    let activate_new = has_flag(args, "--activate-new");

    let existing_catalog = InstrumentCatalog::from_file(&universe_path)?;
    println!(
        "Sync started. Source: {}. Existing instruments: {}",
        source,
        existing_catalog.instruments.len()
    );

    let discovered = discover_instruments_from_source(&source).await?;
    let (merged_catalog, stats, mut new_symbols) =
        merge_catalog_with_discovery(existing_catalog, discovered, activate_new);

    println!(
        "Sync summary: existing={}, discovered={}, present={}, new={}, reclassified={}",
        stats.existing_count,
        stats.discovered_count,
        stats.present_count,
        stats.new_count,
        stats.reclassified_count
    );

    new_symbols.sort();
    if !new_symbols.is_empty() {
        let preview: Vec<String> = new_symbols.iter().take(20).cloned().collect();
        println!(
            "New symbols (first {}): {}",
            preview.len(),
            preview.join(", ")
        );
    }

    if dry_run {
        println!("Dry-run mode: no file changes written to {}", universe_path);
        return Ok(());
    }

    write_catalog_file(&universe_path, &merged_catalog)?;
    println!(
        "Universe updated at {}. Total instruments: {}",
        universe_path,
        merged_catalog.instruments.len()
    );

    Ok(())
}

async fn discover_instruments_from_source(
    source: &str,
) -> Result<Vec<InstrumentDefinition>, Box<dyn std::error::Error>> {
    let normalized_source = source.trim_end_matches('/').to_string();
    let sitemap_url = format!("{}/sitemap.xml", normalized_source);
    let sitemap = reqwest::get(&sitemap_url)
        .await?
        .error_for_status()?
        .text()
        .await?;

    let instrument_slugs = extract_slugs_from_sitemap(&sitemap, "/instrument/");
    let category_slugs = extract_slugs_from_sitemap(&sitemap, "/instruments/");

    if instrument_slugs.is_empty() {
        return Err(format!(
            "No instrument slugs discovered from sitemap at '{}'",
            sitemap_url
        )
        .into());
    }

    let category_membership = fetch_category_membership(&normalized_source, &category_slugs).await;
    let mut discovered = Vec::with_capacity(instrument_slugs.len());

    for slug in &instrument_slugs {
        let Some((base, quote)) = split_symbol_slug(slug) else {
            continue;
        };

        let symbol = format!("{}{}", base, quote);
        let config = resolve_instrument_config(&base, &quote);
        let categories = category_membership.get(slug);
        let asset_class = infer_asset_class(categories, &base, &quote);

        discovered.push(InstrumentDefinition {
            symbol,
            base,
            quote,
            asset_class,
            price_divisor: config.price_divisor,
            decimal_places: config.decimal_places,
            active: false,
        });
    }

    discovered.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    discovered.dedup_by(|left, right| left.symbol == right.symbol);

    Ok(discovered)
}

async fn fetch_category_membership(
    source: &str,
    categories: &[String],
) -> HashMap<String, HashSet<String>> {
    if categories.is_empty() {
        return HashMap::new();
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            eprintln!(
                "  warning: failed to create HTTP client for categories: {}",
                err
            );
            return HashMap::new();
        }
    };

    let max_in_flight = CATEGORY_FETCH_CONCURRENCY.max(1).min(categories.len());
    let mut pending = categories.iter().cloned();
    let mut join_set: JoinSet<Result<(String, Vec<String>), String>> = JoinSet::new();

    for _ in 0..max_in_flight {
        if let Some(category) = pending.next() {
            spawn_category_fetch_job(&mut join_set, client.clone(), source.to_string(), category);
        }
    }

    let mut membership: HashMap<String, HashSet<String>> = HashMap::new();
    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(Ok((category, slugs))) => {
                for slug in slugs {
                    membership.entry(slug).or_default().insert(category.clone());
                }
            }
            Ok(Err(message)) => eprintln!("  warning: {}", message),
            Err(err) => eprintln!("  warning: category fetch worker failed: {}", err),
        }

        if let Some(category) = pending.next() {
            spawn_category_fetch_job(&mut join_set, client.clone(), source.to_string(), category);
        }
    }

    membership
}

fn spawn_category_fetch_job(
    join_set: &mut JoinSet<Result<(String, Vec<String>), String>>,
    client: reqwest::Client,
    source: String,
    category: String,
) {
    join_set.spawn(async move {
        let url = format!("{}/instruments/{}", source.trim_end_matches('/'), category);
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|err| format!("Failed to fetch category '{}': {}", category, err))?;

        if !response.status().is_success() {
            return Err(format!(
                "Category '{}' returned unexpected status {}",
                category,
                response.status()
            ));
        }

        let html = response
            .text()
            .await
            .map_err(|err| format!("Failed to read category '{}' body: {}", category, err))?;
        let slugs = extract_instrument_slugs_from_html(&html);

        if slugs.is_empty() {
            return Err(format!(
                "No instrument links extracted for category '{}'",
                category
            ));
        }

        Ok((category, slugs))
    });
}

fn merge_catalog_with_discovery(
    existing: InstrumentCatalog,
    discovered: Vec<InstrumentDefinition>,
    activate_new: bool,
) -> (InstrumentCatalog, SyncUniverseStats, Vec<String>) {
    let mut merged = existing;
    let mut stats = SyncUniverseStats {
        existing_count: merged.instruments.len(),
        discovered_count: discovered.len(),
        ..SyncUniverseStats::default()
    };

    let mut symbol_index: HashMap<String, usize> = HashMap::new();
    for (index, instrument) in merged.instruments.iter().enumerate() {
        symbol_index.insert(instrument.symbol.to_ascii_uppercase(), index);
    }

    let mut new_symbols = Vec::new();
    for mut instrument in discovered {
        let symbol = instrument.symbol.to_ascii_uppercase();

        if let Some(index) = symbol_index.get(&symbol).copied() {
            stats.present_count += 1;

            let existing_instrument = &mut merged.instruments[index];
            if matches!(existing_instrument.asset_class, AssetClass::Other)
                && !matches!(instrument.asset_class, AssetClass::Other)
            {
                existing_instrument.asset_class = instrument.asset_class;
                stats.reclassified_count += 1;
            }

            continue;
        }

        instrument.active = activate_new;
        symbol_index.insert(symbol.clone(), merged.instruments.len());
        merged.instruments.push(instrument);
        new_symbols.push(symbol);
        stats.new_count += 1;
    }

    merged
        .instruments
        .sort_by(|left, right| left.symbol.cmp(&right.symbol));

    (merged, stats, new_symbols)
}

fn write_catalog_file(
    path: &str,
    catalog: &InstrumentCatalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut instruments = catalog.instruments.clone();
    instruments.sort_by(|left, right| left.symbol.cmp(&right.symbol));

    let mut code_aliases = BTreeMap::new();
    for (alias, canonical) in &catalog.code_aliases {
        let alias = alias.trim().to_ascii_uppercase();
        let canonical = canonical.trim().to_ascii_uppercase();
        if alias.is_empty() || canonical.is_empty() {
            continue;
        }
        code_aliases.insert(alias, canonical);
    }

    let persisted = PersistedCatalog {
        instruments,
        code_aliases,
    };
    let json = serde_json::to_string_pretty(&persisted)?;
    InstrumentCatalog::from_json_str(&json)?;
    fs::write(path, format!("{}\n", json))?;
    Ok(())
}

fn infer_asset_class(categories: Option<&HashSet<String>>, base: &str, quote: &str) -> AssetClass {
    if let Some(categories) = categories {
        if categories.contains("fx_metals") {
            return AssetClass::Metal;
        }
        if categories.contains("fx_majors") || categories.contains("fx_crosses") {
            return AssetClass::Fx;
        }
        if categories
            .iter()
            .any(|category| category.starts_with("idx_"))
        {
            return AssetClass::Index;
        }
        if categories.contains("vccy") {
            return AssetClass::Crypto;
        }
        if categories
            .iter()
            .any(|category| category.starts_with("cmd_"))
        {
            return AssetClass::Commodity;
        }
        if categories
            .iter()
            .any(|category| category.starts_with("etf_cfd") || is_country_equity_category(category))
        {
            return AssetClass::Equity;
        }
    }

    if matches!(base, "XAU" | "XAG" | "XPT" | "XPD")
        || matches!(quote, "XAU" | "XAG" | "XPT" | "XPD")
    {
        return AssetClass::Metal;
    }

    if base.contains("IDX") || quote.contains("IDX") || base.chars().any(|ch| ch.is_ascii_digit()) {
        return AssetClass::Index;
    }

    if base.len() == 3
        && quote.len() == 3
        && base.chars().all(|ch| ch.is_ascii_uppercase())
        && quote.chars().all(|ch| ch.is_ascii_uppercase())
    {
        return AssetClass::Fx;
    }

    AssetClass::Other
}

fn is_country_equity_category(category: &str) -> bool {
    matches!(
        category,
        "austria"
            | "belgium"
            | "denmark"
            | "finland"
            | "france"
            | "germany"
            | "hong-kong"
            | "ireland"
            | "italy"
            | "japan"
            | "mexico"
            | "netherlands"
            | "norway"
            | "portugal"
            | "spain"
            | "sweden"
            | "switzerland"
            | "uk"
            | "us"
    )
}

fn extract_slugs_from_sitemap(xml: &str, marker: &str) -> Vec<String> {
    let mut slugs = BTreeSet::new();
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut in_loc = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag)) if tag.name().as_ref() == b"loc" => {
                in_loc = true;
            }
            Ok(Event::End(tag)) if tag.name().as_ref() == b"loc" => {
                in_loc = false;
            }
            Ok(Event::Text(text)) if in_loc => {
                let loc_value = String::from_utf8_lossy(text.as_ref()).into_owned();
                if let Some(marker_position) = loc_value.find(marker) {
                    let rest = &loc_value[marker_position + marker.len()..];
                    let slug = rest
                        .split(['?', '#', '/'])
                        .next()
                        .map(str::trim)
                        .unwrap_or("");
                    if !slug.is_empty()
                        && slug
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
                    {
                        slugs.insert(slug.to_ascii_lowercase());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }

    slugs.into_iter().collect()
}

fn extract_instrument_slugs_from_html(html: &str) -> Vec<String> {
    let mut slugs = BTreeSet::new();
    let document = Html::parse_document(html);
    let Ok(selector) = Selector::parse("a[href]") else {
        return Vec::new();
    };

    for element in document.select(&selector) {
        let Some(href) = element.value().attr("href") else {
            continue;
        };
        let Some(marker_idx) = href.find("/instrument/") else {
            continue;
        };

        let rest = &href[marker_idx + "/instrument/".len()..];
        let slug = rest
            .split(['/', '?', '#'])
            .next()
            .map(str::trim)
            .unwrap_or("");
        if slug.len() >= 4
            && slug
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            slugs.insert(slug.to_ascii_lowercase());
        }
    }

    slugs.into_iter().collect()
}

fn validate_output_mode(
    out_path: Option<&str>,
    no_output: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    if out_path.is_some() && no_output {
        return Err("Use either --out PATH or --no-output, not both.".into());
    }
    if out_path.is_none() && !no_output {
        return Err("Missing output mode. Provide --out PATH or --no-output.".into());
    }

    Ok(out_path.is_some())
}

fn should_persist_checkpoints(persist_output: bool) -> bool {
    if persist_output {
        true
    } else {
        eprintln!(
            "  warning: --no-output selected, checkpoint updates are disabled to avoid data-loss traps"
        );
        false
    }
}

fn build_tickers(
    instruments: &[&InstrumentDefinition],
    interval: Duration,
) -> Result<Vec<Ticker>, DukascopyError> {
    instruments
        .iter()
        .map(|instrument| {
            Ticker::try_new(&instrument.base, &instrument.quote)
                .map(|ticker| ticker.interval(interval))
        })
        .collect()
}

fn build_client_from_catalog(
    catalog: &InstrumentCatalog,
    instruments: &[&InstrumentDefinition],
    concurrency: usize,
) -> ConfiguredClient {
    let max_concurrency = concurrency.max(1);
    DukascopyClientBuilder::new()
        .respect_market_hours(should_respect_market_hours(instruments))
        .max_in_flight_requests(max_concurrency)
        .max_download_concurrency(max_concurrency)
        .with_instrument_catalog(catalog)
        .build()
}

fn should_respect_market_hours(instruments: &[&InstrumentDefinition]) -> bool {
    instruments
        .iter()
        .all(|instrument| matches!(instrument.asset_class, AssetClass::Fx | AssetClass::Metal))
}

fn create_sink(path: Option<&str>) -> Result<Box<dyn DataSink>, Box<dyn std::error::Error>> {
    match path {
        Some(path) => {
            let path_lower = path.to_ascii_lowercase();
            if path_lower.ends_with(".csv") {
                return Ok(Box::new(CsvSink::open(path)?));
            }
            if path_lower.ends_with(".parquet") {
                #[cfg(feature = "sinks-parquet")]
                {
                    return Ok(Box::new(ParquetSink::open(path)?));
                }
                #[cfg(not(feature = "sinks-parquet"))]
                {
                    return Err(
                        "Parquet sink requires 'sinks-parquet' feature. Rebuild with --features sinks-parquet"
                            .into(),
                    );
                }
            }
            Err(format!(
                "Unsupported sink format for '{}'. Use .csv{}",
                path,
                if cfg!(feature = "sinks-parquet") {
                    " or .parquet"
                } else {
                    ""
                }
            )
            .into())
        }
        None => Ok(Box::new(NoopSink)),
    }
}

fn split_symbol_slug(slug: &str) -> Option<(String, String)> {
    let normalized = slug.trim().to_ascii_uppercase();
    if normalized.len() < 6 {
        return None;
    }
    if !normalized.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return None;
    }

    let quote_len = 3usize;
    if normalized.len() <= quote_len {
        return None;
    }

    let split = normalized.len() - quote_len;
    let base = &normalized[..split];
    let quote = &normalized[split..];

    if !(2..=12).contains(&base.len()) {
        return None;
    }
    if quote.len() != 3 {
        return None;
    }

    Some((base.to_string(), quote.to_string()))
}

async fn run_backfill(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if has_flag(args, "--help") {
        print_backfill_usage();
        return Ok(());
    }
    validate_flags(
        args,
        &[
            "--universe",
            "--symbols",
            "--period",
            "--interval",
            "--checkpoint",
            "--out",
            "--concurrency",
        ],
        &["--no-output"],
    )?;

    let universe_path =
        read_flag_value(args, "--universe").unwrap_or_else(|| DEFAULT_UNIVERSE_PATH.to_string());
    let checkpoint_path = read_flag_value(args, "--checkpoint")
        .unwrap_or_else(|| DEFAULT_CHECKPOINT_PATH.to_string());
    let symbols = parse_symbol_list(read_flag_value(args, "--symbols"));
    let period = read_flag_value(args, "--period").unwrap_or_else(|| "30d".to_string());
    let interval =
        parse_duration(&read_flag_value(args, "--interval").unwrap_or_else(|| "1h".to_string()))?;
    let out_path = read_flag_value(args, "--out");
    let no_output = has_flag(args, "--no-output");
    let concurrency =
        parse_positive_usize(read_flag_value(args, "--concurrency"), DEFAULT_CONCURRENCY)?;
    let persist_output = validate_output_mode(out_path.as_deref(), no_output)?;
    let persist_checkpoints = should_persist_checkpoints(persist_output);

    let catalog = InstrumentCatalog::from_file(&universe_path)?;
    let selected = catalog.select_active(&symbols)?;
    let tickers = build_tickers(&selected, interval)?;
    let client = Arc::new(build_client_from_catalog(&catalog, &selected, concurrency));
    let checkpoint_store = FileCheckpointStore::open(&checkpoint_path)?;
    let mut sink = create_sink(out_path.as_deref())?;

    println!(
        "Backfill started for {} instruments (period={}, interval={}s, concurrency={})",
        tickers.len(),
        period,
        interval.num_seconds(),
        concurrency
    );

    let mut results = fetch_backfill_batches(&tickers, client, &period, concurrency).await;
    results.sort_by_key(|result| result.ticker.symbol());

    let mut total_rows = 0usize;
    let mut checkpoint_updates: Vec<(String, DateTime<Utc>)> = Vec::new();
    let mut failures: Vec<(String, String)> = Vec::new();

    for result in results {
        let symbol = result.ticker.symbol();
        match result.rows {
            Ok(history) => {
                if let Some(last) = history.last() {
                    checkpoint_updates.push((result.ticker.checkpoint_key(), last.timestamp));
                }
                let _ = sink.write_batch(&symbol, &history)?;
                total_rows += history.len();
                println!("  {} -> {} rows", symbol, history.len());
            }
            Err(err) => {
                eprintln!("  {} -> error: {}", symbol, err);
                failures.push((symbol, err.to_string()));
            }
        }
    }

    if persist_checkpoints && !checkpoint_updates.is_empty() {
        checkpoint_store.set_many(&checkpoint_updates)?;
    }
    sink.flush()?;

    println!(
        "Backfill finished. Total rows: {}. Checkpoint file: {}. Errors: {}",
        total_rows,
        checkpoint_path,
        failures.len()
    );

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Backfill completed with {} failed instrument(s)",
            failures.len()
        )
        .into())
    }
}

async fn run_update(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if has_flag(args, "--help") {
        print_update_usage();
        return Ok(());
    }
    validate_flags(
        args,
        &[
            "--universe",
            "--symbols",
            "--lookback",
            "--interval",
            "--checkpoint",
            "--out",
            "--concurrency",
        ],
        &["--no-output"],
    )?;

    let universe_path =
        read_flag_value(args, "--universe").unwrap_or_else(|| DEFAULT_UNIVERSE_PATH.to_string());
    let checkpoint_path = read_flag_value(args, "--checkpoint")
        .unwrap_or_else(|| DEFAULT_CHECKPOINT_PATH.to_string());
    let symbols = parse_symbol_list(read_flag_value(args, "--symbols"));
    let lookback =
        parse_duration(&read_flag_value(args, "--lookback").unwrap_or_else(|| "7d".to_string()))?;
    let interval =
        parse_duration(&read_flag_value(args, "--interval").unwrap_or_else(|| "1h".to_string()))?;
    let out_path = read_flag_value(args, "--out");
    let no_output = has_flag(args, "--no-output");
    let concurrency =
        parse_positive_usize(read_flag_value(args, "--concurrency"), DEFAULT_CONCURRENCY)?;
    let persist_output = validate_output_mode(out_path.as_deref(), no_output)?;
    let persist_checkpoints = should_persist_checkpoints(persist_output);

    let catalog = InstrumentCatalog::from_file(&universe_path)?;
    let selected = catalog.select_active(&symbols)?;
    let tickers = build_tickers(&selected, interval)?;
    let client = Arc::new(build_client_from_catalog(&catalog, &selected, concurrency));
    let checkpoint_store = FileCheckpointStore::open(&checkpoint_path)?;
    let mut sink = create_sink(out_path.as_deref())?;

    println!(
        "Incremental update started for {} instruments (lookback={}s, interval={}s, concurrency={})",
        tickers.len(),
        lookback.num_seconds(),
        interval.num_seconds(),
        concurrency
    );

    let (jobs, skipped) = build_update_jobs(&tickers, &checkpoint_store, lookback)?;
    for ticker in skipped {
        println!("  {} -> 0 rows (up-to-date)", ticker.symbol());
    }

    let mut results = fetch_update_batches(jobs, client, concurrency).await;
    results.sort_by_key(|result| result.ticker.symbol());

    let mut total_rows = 0usize;
    let mut checkpoint_updates: Vec<(String, DateTime<Utc>)> = Vec::new();
    let mut failures: Vec<(String, String)> = Vec::new();

    for result in results {
        let symbol = result.ticker.symbol();
        match result.rows {
            Ok(rows) => {
                if let Some(last) = rows.last() {
                    checkpoint_updates.push((result.checkpoint_key, last.timestamp));
                }
                let _ = sink.write_batch(&symbol, &rows)?;
                total_rows += rows.len();
                println!("  {} -> {} rows", symbol, rows.len());
            }
            Err(err) => {
                eprintln!("  {} -> error: {}", symbol, err);
                failures.push((symbol, err.to_string()));
            }
        }
    }

    if persist_checkpoints && !checkpoint_updates.is_empty() {
        checkpoint_store.set_many(&checkpoint_updates)?;
    }
    sink.flush()?;

    println!(
        "Incremental update finished. Total rows: {}. Checkpoint file: {}. Errors: {}",
        total_rows,
        checkpoint_path,
        failures.len()
    );

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Incremental update completed with {} failed instrument(s)",
            failures.len()
        )
        .into())
    }
}

async fn fetch_backfill_batches(
    tickers: &[Ticker],
    client: Arc<ConfiguredClient>,
    period: &str,
    concurrency: usize,
) -> Vec<BackfillFetchResult> {
    let max_in_flight = concurrency.max(1).min(tickers.len().max(1));
    let mut join_set = JoinSet::new();
    let mut pending = tickers.iter().cloned();

    for _ in 0..max_in_flight {
        if let Some(ticker) = pending.next() {
            spawn_backfill_job(
                &mut join_set,
                ticker,
                Arc::clone(&client),
                period.to_string(),
            );
        }
    }

    let mut results = Vec::with_capacity(tickers.len());
    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(result) => results.push(result),
            Err(err) => eprintln!("  worker task failed: {}", err),
        }

        if let Some(ticker) = pending.next() {
            spawn_backfill_job(
                &mut join_set,
                ticker,
                Arc::clone(&client),
                period.to_string(),
            );
        }
    }

    results
}

fn spawn_backfill_job(
    join_set: &mut JoinSet<BackfillFetchResult>,
    ticker: Ticker,
    client: Arc<ConfiguredClient>,
    period: String,
) {
    join_set.spawn(async move {
        let rows = ticker.history_with_client(&client, &period).await;
        BackfillFetchResult { ticker, rows }
    });
}

fn build_update_jobs<S: CheckpointStore>(
    tickers: &[Ticker],
    store: &S,
    lookback: Duration,
) -> Result<(Vec<UpdateJob>, Vec<Ticker>), DukascopyError> {
    let end = last_available_tick_time(Utc::now() - Duration::hours(1));
    let mut jobs = Vec::with_capacity(tickers.len());
    let mut skipped = Vec::new();

    for ticker in tickers {
        let checkpoint_key = ticker.checkpoint_key();
        let retry_buffer = ticker.interval_value() + ticker.interval_value();
        let start = match store.get(&checkpoint_key)? {
            Some(last_timestamp) => last_timestamp - retry_buffer,
            None => end - lookback,
        };

        if start >= end {
            skipped.push(ticker.clone());
            continue;
        }

        jobs.push(UpdateJob {
            ticker: ticker.clone(),
            checkpoint_key,
            start,
            end,
        });
    }

    Ok((jobs, skipped))
}

async fn fetch_update_batches(
    jobs: Vec<UpdateJob>,
    client: Arc<ConfiguredClient>,
    concurrency: usize,
) -> Vec<UpdateFetchResult> {
    if jobs.is_empty() {
        return Vec::new();
    }

    let max_in_flight = concurrency.max(1).min(jobs.len());
    let mut join_set = JoinSet::new();
    let mut pending = jobs.into_iter();

    for _ in 0..max_in_flight {
        if let Some(job) = pending.next() {
            spawn_update_job(&mut join_set, job, Arc::clone(&client));
        }
    }

    let mut results = Vec::new();
    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(result) => results.push(result),
            Err(err) => eprintln!("  worker task failed: {}", err),
        }

        if let Some(job) = pending.next() {
            spawn_update_job(&mut join_set, job, Arc::clone(&client));
        }
    }

    results
}

fn spawn_update_job(
    join_set: &mut JoinSet<UpdateFetchResult>,
    job: UpdateJob,
    client: Arc<ConfiguredClient>,
) {
    join_set.spawn(async move {
        let rows = job
            .ticker
            .history_range_with_client(&client, job.start, job.end)
            .await
            .map(deduplicate_by_timestamp);

        UpdateFetchResult {
            ticker: job.ticker,
            checkpoint_key: job.checkpoint_key,
            rows,
        }
    });
}

#[cfg(feature = "sinks-parquet")]
fn run_export(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if has_flag(args, "--help") {
        print_export_usage();
        return Ok(());
    }
    validate_flags(args, &["--input", "--out"], &["--has-headers"])?;

    let input = read_flag_value(args, "--input")
        .ok_or_else(|| "Missing required argument: --input PATH.csv".to_string())?;
    let out = read_flag_value(args, "--out")
        .ok_or_else(|| "Missing required argument: --out PATH.parquet".to_string())?;
    let has_headers = has_flag(args, "--has-headers");

    if !input.to_ascii_lowercase().ends_with(".csv") {
        return Err(format!("Unsupported export input '{}'. Expected .csv", input).into());
    }
    if !out.to_ascii_lowercase().ends_with(".parquet") {
        return Err(format!("Unsupported export output '{}'. Expected .parquet", out).into());
    }

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(has_headers)
        .from_path(&input)?;
    let mut sink = ParquetSink::open(&out)?;

    let mut total_rows = 0usize;
    for (line_no, record_result) in reader.records().enumerate() {
        let physical_line_no = line_no + if has_headers { 2 } else { 1 };
        let record = record_result.map_err(|err| {
            format!(
                "Failed to read CSV record at line {} from '{}': {}",
                physical_line_no, input, err
            )
        })?;

        if record.len() != 9 {
            return Err(format!(
                "Invalid CSV row at line {} in '{}': expected 9 columns, got {}",
                physical_line_no,
                input,
                record.len()
            )
            .into());
        }

        let symbol = record[0].to_string();
        let pair =
            CurrencyPair::try_new(record[1].to_string(), record[2].to_string()).map_err(|err| {
                format!(
                    "Invalid pair at line {} in '{}': {}",
                    physical_line_no, input, err
                )
            })?;
        let timestamp = chrono::DateTime::parse_from_rfc3339(&record[3])
            .map_err(|err| {
                format!(
                    "Invalid timestamp at line {} in '{}': {}",
                    physical_line_no, input, err
                )
            })?
            .with_timezone(&chrono::Utc);

        let rate = Decimal::from_str(&record[4]).map_err(|err| {
            format!(
                "Invalid rate at line {} in '{}': {}",
                physical_line_no, input, err
            )
        })?;
        let bid = Decimal::from_str(&record[5]).map_err(|err| {
            format!(
                "Invalid bid at line {} in '{}': {}",
                physical_line_no, input, err
            )
        })?;
        let ask = Decimal::from_str(&record[6]).map_err(|err| {
            format!(
                "Invalid ask at line {} in '{}': {}",
                physical_line_no, input, err
            )
        })?;
        let bid_volume: f32 = record[7].parse().map_err(|err| {
            format!(
                "Invalid bid_volume at line {} in '{}': {}",
                physical_line_no, input, err
            )
        })?;
        let ask_volume: f32 = record[8].parse().map_err(|err| {
            format!(
                "Invalid ask_volume at line {} in '{}': {}",
                physical_line_no, input, err
            )
        })?;

        let exchange = CurrencyExchange {
            pair,
            rate,
            timestamp,
            ask,
            bid,
            ask_volume,
            bid_volume,
        };

        let _ = sink.write_batch(&symbol, &[exchange])?;
        total_rows += 1;
    }

    sink.flush()?;
    println!(
        "Export complete. Input: {}. Output: {}. Rows: {}",
        input, out, total_rows
    );
    Ok(())
}

#[cfg(not(feature = "sinks-parquet"))]
fn run_export(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if has_flag(args, "--help") {
        print_export_usage();
        return Ok(());
    }

    let _ = args;
    Err(
        "Export command requires 'sinks-parquet' feature. Rebuild with --features sinks-parquet"
            .into(),
    )
}

fn deduplicate_by_timestamp(mut history: Vec<CurrencyExchange>) -> Vec<CurrencyExchange> {
    history.sort_by_key(|rate| rate.timestamp);
    history.dedup_by_key(|rate| rate.timestamp);
    history
}

fn parse_symbol_list(value: Option<String>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_uppercase())
        .collect()
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn read_flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == flag {
            if i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
            return None;
        }
        i += 1;
    }
    None
}

fn parse_positive_usize(
    value: Option<String>,
    default_value: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
    let Some(raw) = value else {
        return Ok(default_value.max(1));
    };

    let parsed: usize = raw
        .parse()
        .map_err(|_| format!("Invalid positive integer '{}'", raw))?;

    if parsed == 0 {
        return Err("Value must be greater than 0".into());
    }

    Ok(parsed)
}

fn parse_duration(value: &str) -> Result<Duration, Box<dyn std::error::Error>> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() < 2 {
        return Err(format!("Invalid duration '{}'", value).into());
    }

    let (num_str, unit) = if normalized.ends_with("mo") {
        normalized.split_at(normalized.len() - 2)
    } else {
        normalized.split_at(normalized.len() - 1)
    };
    let amount: i64 = num_str
        .parse()
        .map_err(|_| format!("Invalid duration '{}'", value))?;

    if amount <= 0 {
        return Err(format!("Duration must be positive: '{}'", value).into());
    }

    let duration = match unit {
        "m" => Duration::minutes(amount),
        "h" => Duration::hours(amount),
        "d" => Duration::days(amount),
        "w" => Duration::weeks(amount),
        "mo" => Duration::days(amount * 30),
        "y" => Duration::days(amount * 365),
        _ => return Err(format!("Unsupported duration unit in '{}'", value).into()),
    };

    Ok(duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_supports_multiple_units() {
        assert_eq!(parse_duration("30m").unwrap(), Duration::minutes(30));
        assert_eq!(parse_duration("2h").unwrap(), Duration::hours(2));
        assert_eq!(parse_duration("7d").unwrap(), Duration::days(7));
        assert_eq!(parse_duration("2w").unwrap(), Duration::weeks(2));
        assert_eq!(parse_duration("1mo").unwrap(), Duration::days(30));
        assert_eq!(parse_duration("1y").unwrap(), Duration::days(365));
    }

    #[test]
    fn test_parse_duration_rejects_invalid_values() {
        assert!(parse_duration("0h").is_err());
        assert!(parse_duration("-1h").is_err());
        assert!(parse_duration("1x").is_err());
        assert!(parse_duration("abc").is_err());
    }

    #[test]
    fn test_parse_positive_usize_defaults_and_validation() {
        assert_eq!(parse_positive_usize(None, 8).unwrap(), 8);
        assert_eq!(parse_positive_usize(Some("4".to_string()), 8).unwrap(), 4);
        assert!(parse_positive_usize(Some("0".to_string()), 8).is_err());
        assert!(parse_positive_usize(Some("x".to_string()), 8).is_err());
    }

    #[test]
    fn test_validate_flags_rejects_unknown_and_missing_values() {
        let args = vec!["--unknown".to_string()];
        assert!(validate_flags(&args, &["--out"], &[]).is_err());

        let args = vec!["--out".to_string()];
        assert!(validate_flags(&args, &["--out"], &[]).is_err());

        let args = vec!["--out".to_string(), "--next".to_string()];
        assert!(validate_flags(&args, &["--out"], &["--next"]).is_err());
    }

    #[test]
    fn test_validate_output_mode_rules() {
        assert!(validate_output_mode(Some("data.csv"), false).unwrap());
        assert!(!validate_output_mode(None, true).unwrap());
        assert!(validate_output_mode(Some("data.csv"), true).is_err());
        assert!(validate_output_mode(None, false).is_err());
    }

    #[test]
    fn test_deduplicate_by_timestamp_keeps_unique_rows() {
        let ts = Utc::now();
        let rows = vec![
            CurrencyExchange {
                pair: CurrencyPair::new("EUR", "USD"),
                rate: Decimal::from_str("1.10000").unwrap(),
                timestamp: ts,
                ask: Decimal::from_str("1.10010").unwrap(),
                bid: Decimal::from_str("1.09990").unwrap(),
                ask_volume: 1.0,
                bid_volume: 1.0,
            },
            CurrencyExchange {
                pair: CurrencyPair::new("EUR", "USD"),
                rate: Decimal::from_str("1.10000").unwrap(),
                timestamp: ts,
                ask: Decimal::from_str("1.10010").unwrap(),
                bid: Decimal::from_str("1.09990").unwrap(),
                ask_volume: 1.0,
                bid_volume: 1.0,
            },
        ];

        let deduped = deduplicate_by_timestamp(rows);
        assert_eq!(deduped.len(), 1);
    }

    #[test]
    fn test_extract_slugs_from_sitemap() {
        let xml = r#"
        <urlset>
          <url><loc>https://www.dukascopy-node.app/instruments/fx_majors</loc></url>
          <url><loc>https://www.dukascopy-node.app/instrument/eurusd</loc></url>
          <url><loc>https://www.dukascopy-node.app/instrument/aaplususd</loc></url>
        </urlset>
        "#;

        let instruments = extract_slugs_from_sitemap(xml, "/instrument/");
        let categories = extract_slugs_from_sitemap(xml, "/instruments/");
        assert_eq!(instruments, vec!["aaplususd", "eurusd"]);
        assert_eq!(categories, vec!["fx_majors"]);
    }

    #[test]
    fn test_extract_instrument_slugs_from_html() {
        let html = r#"
        <a href="/instrument/eurusd">EUR/USD</a>
        <a href="/instrument/aaplususd">AAPL</a>
        <a href="/instrument/eurusd">duplicate</a>
        "#;

        let slugs = extract_instrument_slugs_from_html(html);
        assert_eq!(slugs, vec!["aaplususd", "eurusd"]);
    }

    #[test]
    fn test_split_symbol_slug() {
        assert_eq!(
            split_symbol_slug("eurusd"),
            Some(("EUR".to_string(), "USD".to_string()))
        );
        assert_eq!(
            split_symbol_slug("aaplususd"),
            Some(("AAPLUS".to_string(), "USD".to_string()))
        );
        assert!(split_symbol_slug("bad").is_none());
    }

    #[test]
    fn test_infer_asset_class_prefers_category_mapping() {
        let mut categories = HashSet::new();
        categories.insert("fx_majors".to_string());
        assert_eq!(
            infer_asset_class(Some(&categories), "EUR", "USD"),
            AssetClass::Fx
        );

        categories.clear();
        categories.insert("idx_america".to_string());
        assert_eq!(
            infer_asset_class(Some(&categories), "USA500IDX", "USD"),
            AssetClass::Index
        );

        categories.clear();
        categories.insert("etf_cfd_us".to_string());
        assert_eq!(
            infer_asset_class(Some(&categories), "SPYUS", "USD"),
            AssetClass::Equity
        );
    }

    #[test]
    fn test_merge_catalog_with_discovery_adds_new_as_inactive_by_default() {
        let existing = InstrumentCatalog {
            instruments: vec![InstrumentDefinition {
                symbol: "EURUSD".to_string(),
                base: "EUR".to_string(),
                quote: "USD".to_string(),
                asset_class: AssetClass::Fx,
                price_divisor: 100000.0,
                decimal_places: 5,
                active: true,
            }],
            code_aliases: HashMap::new(),
        };

        let discovered = vec![
            InstrumentDefinition {
                symbol: "EURUSD".to_string(),
                base: "EUR".to_string(),
                quote: "USD".to_string(),
                asset_class: AssetClass::Fx,
                price_divisor: 100000.0,
                decimal_places: 5,
                active: false,
            },
            InstrumentDefinition {
                symbol: "AAPLUSUSD".to_string(),
                base: "AAPLUS".to_string(),
                quote: "USD".to_string(),
                asset_class: AssetClass::Equity,
                price_divisor: 1000.0,
                decimal_places: 2,
                active: false,
            },
        ];

        let (merged, stats, new_symbols) =
            merge_catalog_with_discovery(existing, discovered, false);
        assert_eq!(stats.existing_count, 1);
        assert_eq!(stats.present_count, 1);
        assert_eq!(stats.new_count, 1);
        assert_eq!(new_symbols, vec!["AAPLUSUSD".to_string()]);
        assert_eq!(merged.instruments.len(), 2);

        let aapl = merged
            .instruments
            .iter()
            .find(|instrument| instrument.symbol == "AAPLUSUSD")
            .unwrap();
        assert!(!aapl.active);
    }
}
