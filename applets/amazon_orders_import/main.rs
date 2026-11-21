//! Amazon orders import: one-time ingest of Amazon's order history into
//! `data_financial_purchase`, one row per item ordered.
//!
//! Manual-trigger only (no cron). `POST /api/imports/amazon/upload` stages
//! the upload and starts this applet with `payload = { "file_path": "..." }`.
//!
//! The file is the zip from Amazon's "Request your data" page, or the order
//! history CSV from inside it. The zip may hold several numbered order
//! histories (`Retail.OrderHistory.1`, `.2`, ...), and every one is read. The
//! CSV is read one record at a time on a blocking thread and written in
//! batches, so memory stays flat.
//!
//! Rows dedup on `source_stream_id`, so importing a newer download later adds
//! only the new orders (`ON CONFLICT DO NOTHING`).

mod transform;

use std::path::Path;

use anyhow::{bail, Context, Result};
use sqlx::PgPool;
use transform::{is_order_history_file, parse_line, Layout, LineIndex, Purchase};
use virtues_applets::import::{self, base_name, Batches, Csv, Tally};
use virtues_helpers::dedup::build_batch_insert_query;
use virtues_helpers::{connect_from_env, output_with_records, read_input};

const APPLET: &str = "amazon_orders_import";

const NOT_A_DOWNLOAD: &str = "This file has no Amazon orders in it. Use the zip from Amazon's Request your data \
     page, or the Retail.OrderHistory CSV inside it.";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();
    let input = read_input()?;
    let path = import::staged_file(&input, APPLET)?;
    let result = run(&path).await;
    import::remove_upload(&path, APPLET);
    let tally = result?;
    if tally.parsed == 0 {
        bail!("{NOT_A_DOWNLOAD}");
    }
    output_with_records(&tally.summary("Amazon orders", "were"), &input.config, tally.written as i64)?;
    Ok(())
}

async fn run(path: &Path) -> Result<Tally> {
    let pool = connect_from_env("virtues-action-amazon_orders_import").await?;
    let (mut rx, reader) = import::spawn_reader(path, read_download);
    let mut tally = Tally::default();
    while let Some(batch) = rx.recv().await {
        for p in &batch {
            tally.saw(p.occurred_at);
        }
        tally.written += write_purchases(&pool, &batch).await?;
    }
    import::join_reader(reader).await?;
    Ok(tally)
}

/// Read every order line in the upload, a zip or a bare CSV, handing them to
/// `sink` in batches.
fn read_download(path: &Path, sink: &mut dyn FnMut(Vec<Purchase>) -> Result<()>) -> Result<()> {
    let mut batches = Batches::new(sink);
    let mut lines = LineIndex::default();
    let mut histories = 0;
    let files = import::for_each_file(path, |n| is_order_history_file(base_name(n)), &mut |name, reader| {
        let mut csv = Csv::new(reader);
        let Some(header) = csv.next_record()? else {
            return Ok(());
        };
        let Some(layout) = Layout::from_header(&header) else {
            tracing::info!("{APPLET}: {name} has no Order ID column; skipping it");
            return Ok(());
        };
        histories += 1;
        let read = (|| -> Result<()> {
            while let Some(record) = csv.next_record()? {
                if let Some(p) = parse_line(&layout, &record, &mut lines) {
                    batches.push(p)?;
                }
            }
            Ok(())
        })();
        // What came before a damaged record is kept.
        batches.flush()?;
        read.context("This file stops partway through. Download it again and retry; the orders before the break were kept.")
    })?;
    if files == 0 || histories == 0 {
        bail!("{NOT_A_DOWNLOAD}");
    }
    Ok(())
}

/// Insert purchases, skipping any already here. Returns how many were new.
async fn write_purchases(pool: &PgPool, rows: &[Purchase]) -> Result<u64> {
    if rows.is_empty() {
        return Ok(0);
    }
    let sql = build_batch_insert_query(
        "data_financial_purchase",
        &[
            "id",
            "occurred_at",
            "order_id",
            "item_name",
            "external_id",
            "quantity",
            "unit_price_cents",
            "total_cents",
            "currency",
            "status",
            "merchant_name",
            "source_stream_id",
            "source_table",
            "source_provider",
            "metadata",
        ],
        "source_stream_id",
        rows.len(),
    );
    let mut q = sqlx::query(&sql);
    for p in rows {
        q = q
            .bind(import::row_id(&p.source_stream_id))
            .bind(p.occurred_at)
            .bind(&p.order_id)
            .bind(&p.item_name)
            .bind(&p.external_id)
            .bind(p.quantity)
            .bind(p.unit_price_cents)
            .bind(p.total_cents)
            .bind(&p.currency)
            .bind(&p.status)
            .bind("Amazon")
            .bind(&p.source_stream_id)
            .bind("amazon_orders_import")
            .bind("amazon")
            .bind(&p.metadata);
    }
    Ok(q.execute(pool).await?.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    const ONE: &str = "Order ID,Order Date,Product Name,ASIN,Quantity,Unit Price,Total Owed,Currency,Order Status\n\
                       111-1,2024-02-10T15:04:05Z,Paper Lanterns,B000000001,1,12.99,14.06,USD,Closed\n\
                       111-1,2024-02-10T15:04:05Z,Example Desk Lamp,B000000002,1,30.00,32.48,USD,Closed\n";
    const TWO: &str = "Order ID,Order Date,Product Name,ASIN,Quantity,Unit Price,Total Owed,Currency,Order Status\n\
                       222-2,2025-06-01T10:00:00Z,Night Market (Vinyl),B000000003,1,24.50,26.52,CAD,Closed\n";

    fn temp_path(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("amazon_import_test_{unique}_{name}"))
    }

    fn collect(path: &Path) -> Result<Vec<Purchase>> {
        let mut out = Vec::new();
        read_download(path, &mut |b| {
            out.extend(b);
            Ok(())
        })?;
        Ok(out)
    }

    #[test]
    fn a_zip_reads_every_numbered_order_history() {
        let path = temp_path("orders.zip");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in [
            ("Retail.OrderHistory.1/Retail.OrderHistory.1.csv", ONE),
            ("Retail.OrderHistory.2/Retail.OrderHistory.2.csv", TWO),
            ("Retail.CustomerReturns.1/Retail.CustomerReturns.1.csv", "Order ID,Return Date\n111-1,2024-03-01\n"),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let rows = collect(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2].currency.as_deref(), Some("CAD"));
    }

    #[test]
    fn a_bare_csv_is_read_and_another_file_refused() {
        let path = temp_path("bare.upload");
        std::fs::write(&path, ONE).unwrap();
        assert_eq!(collect(&path).unwrap().len(), 2);
        std::fs::write(&path, "Date,Amount\n2024-01-01,5\n").unwrap();
        let err = collect(&path).unwrap_err().to_string();
        std::fs::remove_file(&path).unwrap();
        assert!(err.contains("no Amazon orders"), "{err}");
    }

    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_second_import_of_the_same_orders_adds_nothing(pool: PgPool) {
        let path = temp_path("again.upload");
        std::fs::write(&path, ONE).unwrap();
        let rows = collect(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(write_purchases(&pool, &rows).await.unwrap(), 2);
        assert_eq!(write_purchases(&pool, &rows).await.unwrap(), 0);
        let (n, merchant): (i64, String) =
            sqlx::query_as("SELECT count(*), min(merchant_name) FROM data_financial_purchase")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((n, merchant.as_str()), (2, "Amazon"));
    }
}
