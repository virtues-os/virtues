//! One line of an Amazon order history CSV, shaped for
//! `data_financial_purchase`.
//!
//! Amazon has renamed and reordered these columns over the years, so each is
//! found by name, with the older names as aliases. The file also carries the
//! shipping and billing addresses, the payment card, gift messages and their
//! senders and recipients, and tracking numbers. None of those columns are
//! looked up, so none of them are kept.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use virtues_applets::import::{content_key, field, parse_time, Columns};

#[derive(Debug, Clone)]
pub struct Purchase {
    pub occurred_at: DateTime<Utc>,
    pub order_id: String,
    pub item_name: String,
    pub external_id: Option<String>,
    pub quantity: Option<i32>,
    pub unit_price_cents: Option<i64>,
    pub total_cents: Option<i64>,
    pub currency: Option<String>,
    pub status: Option<String>,
    pub source_stream_id: String,
    pub metadata: Value,
}

/// Where each column this import reads sits in one file's header.
pub struct Layout {
    order_id: usize,
    order_date: Option<usize>,
    product: Option<usize>,
    asin: Option<usize>,
    quantity: Option<usize>,
    unit_price: Option<usize>,
    total: Option<usize>,
    currency: Option<usize>,
    order_status: Option<usize>,
    shipment_status: Option<usize>,
    website: Option<usize>,
    ship_date: Option<usize>,
    condition: Option<usize>,
}

impl Layout {
    /// The layout of an order history header, or `None` for a file with no
    /// Order ID column, which is not an order history.
    pub fn from_header(header: &[String]) -> Option<Layout> {
        let c = Columns::new(header);
        Some(Layout {
            order_id: c.find(&["Order ID"])?,
            order_date: c.find(&["Order Date"]),
            product: c.find(&["Product Name", "Title"]),
            asin: c.find(&["ASIN", "ASIN/ISBN"]),
            quantity: c.find(&["Quantity"]),
            unit_price: c.find(&["Unit Price", "Purchase Price Per Unit"]),
            total: c.find(&["Total Owed", "Item Total"]),
            currency: c.find(&["Currency"]),
            order_status: c.find(&["Order Status"]),
            shipment_status: c.find(&["Shipment Status"]),
            website: c.find(&["Website"]),
            ship_date: c.find(&["Ship Date", "Shipment Date"]),
            condition: c.find(&["Product Condition", "Condition"]),
        })
    }
}

/// Numbers the lines of one item within one order, in the order the files
/// list them. The number only tells apart repeated lines of the same item, so
/// a line keeps its key when a later download lists the order's other items
/// in a different order.
#[derive(Default)]
pub struct LineIndex(HashMap<(String, String), usize>);

impl LineIndex {
    fn next(&mut self, order_id: &str, item_key: &str) -> usize {
        let n = self.0.entry((order_id.to_string(), item_key.to_string())).or_insert(0);
        let i = *n;
        *n += 1;
        i
    }
}

/// The purchase one CSV record describes, or `None` for a record with no
/// order id, no date that parses, or nothing naming the item.
pub fn parse_line(layout: &Layout, record: &[String], lines: &mut LineIndex) -> Option<Purchase> {
    let order_id = field(record, Some(layout.order_id))?.to_string();
    let occurred_at = parse_time(field(record, layout.order_date)?)?;
    let asin = field(record, layout.asin).map(str::to_string);
    let item_name = field(record, layout.product).map(str::to_string).or_else(|| asin.clone())?;
    let item_key = asin.clone().unwrap_or_else(|| content_key(&[&item_name]));
    let index = lines.next(&order_id, &item_key);

    let mut metadata = Map::new();
    for (key, col) in [("website", layout.website), ("ship_date", layout.ship_date), ("product_condition", layout.condition)] {
        if let Some(v) = field(record, col).filter(|v| !is_not_available(v)) {
            metadata.insert(key.to_string(), json!(v));
        }
    }

    Some(Purchase {
        occurred_at,
        source_stream_id: format!("amazon:{order_id}:{item_key}:{index}"),
        order_id,
        item_name,
        external_id: asin,
        quantity: field(record, layout.quantity).and_then(|q| q.parse::<i32>().ok()),
        unit_price_cents: field(record, layout.unit_price).and_then(cents),
        total_cents: field(record, layout.total).and_then(cents),
        currency: field(record, layout.currency).filter(|v| !is_not_available(v)).map(str::to_string),
        status: field(record, layout.order_status)
            .or_else(|| field(record, layout.shipment_status))
            .filter(|v| !is_not_available(v))
            .map(str::to_string),
        metadata: Value::Object(metadata),
    })
}

fn is_not_available(v: &str) -> bool {
    v.eq_ignore_ascii_case("not available") || v.eq_ignore_ascii_case("n/a")
}

/// A price as cents: "$1,299.00", "12.99", "-5.00", "12,99". `None` for
/// "Not Available" or anything without digits. The last `.` or `,` is the
/// decimal point when one or two digits follow it; any other separator
/// groups thousands.
pub fn cents(raw: &str) -> Option<i64> {
    let s = raw.trim();
    let negative = s.starts_with('-') || (s.starts_with('(') && s.ends_with(')'));
    let kept: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
    if !kept.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let (whole, frac) = match kept.rfind(['.', ',']) {
        Some(i) if (1..=2).contains(&(kept.len() - i - 1)) => (&kept[..i], &kept[i + 1..]),
        _ => (kept.as_str(), ""),
    };
    let whole: String = whole.chars().filter(char::is_ascii_digit).collect();
    let whole: i64 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
    let frac: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        _ => frac.parse().ok()?,
    };
    let value = whole.checked_mul(100)?.checked_add(frac)?;
    Some(if negative { -value } else { value })
}

/// An order history CSV in the "Request your data" zip:
/// `Retail.OrderHistory.<n>/Retail.OrderHistory.<n>.csv`.
pub fn is_order_history_file(base: &str) -> bool {
    let lower = base.to_ascii_lowercase();
    lower.starts_with("retail.orderhistory") && lower.ends_with(".csv")
}

#[cfg(test)]
mod tests {
    use super::*;
    use virtues_applets::import::Csv;

    const CURRENT: &str = "\"Website\",\"Order ID\",\"Order Date\",\"Purchase Order Number\",\"Currency\",\"Unit Price\",\"Unit Price Tax\",\"Shipping Charge\",\"Total Discounts\",\"Total Owed\",\"Shipment Item Subtotal\",\"Shipment Item Subtotal Tax\",\"ASIN\",\"Product Condition\",\"Quantity\",\"Payment Instrument Type\",\"Order Status\",\"Shipment Status\",\"Ship Date\",\"Shipping Option\",\"Shipping Address\",\"Billing Address\",\"Carrier Name & Tracking Number\",\"Product Name\",\"Gift Message\",\"Gift Sender Name\",\"Gift Recipient Contact Details\",\"Item Serial Number\"
\"Amazon.com\",\"111-0000001-0000001\",\"2024-02-10T15:04:05Z\",\"Not Applicable\",\"USD\",\"12.99\",\"1.07\",\"0\",\"0\",\"14.06\",\"Not Available\",\"Not Available\",\"B000000001\",\"New\",\"1\",\"Visa - 0000\",\"Closed\",\"Shipped\",\"2024-02-11T09:00:00Z\",\"standard\",\"Nick Example 1 Example St Austin TX\",\"Nick Example 1 Example St Austin TX\",\"AMZN_US(TBA000000000000)\",\"Paper Lanterns, set of 4\",\"Happy birthday\",\"Nick\",\"David Okafor\",\"Not Available\"
\"Amazon.com\",\"111-0000001-0000001\",\"2024-02-10T15:04:05Z\",\"Not Applicable\",\"USD\",\"Not Available\",\"0\",\"0\",\"0\",\"$1,299.00\",\"Not Available\",\"Not Available\",\"B000000002\",\"New\",\"2\",\"Visa - 0000\",\"Cancelled\",\"Not Available\",\"Not Available\",\"standard\",\"x\",\"x\",\"x\",\"Example Desk Lamp\",\"\",\"\",\"\",\"\"
";

    fn parse_all(csv: &str) -> Vec<Purchase> {
        let mut csv = Csv::new(csv.as_bytes());
        let layout = Layout::from_header(&csv.next_record().unwrap().unwrap()).unwrap();
        let mut lines = LineIndex::default();
        let mut out = Vec::new();
        while let Some(r) = csv.next_record().unwrap() {
            out.extend(parse_line(&layout, &r, &mut lines));
        }
        out
    }

    #[test]
    fn a_current_order_history_shapes_each_line() {
        let rows = parse_all(CURRENT);
        assert_eq!(rows.len(), 2);
        let a = &rows[0];
        assert_eq!(a.order_id, "111-0000001-0000001");
        assert_eq!(a.item_name, "Paper Lanterns, set of 4");
        assert_eq!(a.external_id.as_deref(), Some("B000000001"));
        assert_eq!((a.quantity, a.unit_price_cents, a.total_cents), (Some(1), Some(1299), Some(1406)));
        assert_eq!(a.currency.as_deref(), Some("USD"));
        assert_eq!(a.status.as_deref(), Some("Closed"));
        assert_eq!(a.occurred_at.to_rfc3339(), "2024-02-10T15:04:05+00:00");
        assert_eq!(a.source_stream_id, "amazon:111-0000001-0000001:B000000001:0");
        assert_eq!(a.metadata["product_condition"], "New");

        let b = &rows[1];
        assert_eq!(b.source_stream_id, "amazon:111-0000001-0000001:B000000002:0");
        assert_eq!((b.unit_price_cents, b.total_cents), (None, Some(129_900)));
        assert_eq!(b.status.as_deref(), Some("Cancelled"));
    }

    #[test]
    fn addresses_payment_gifts_and_tracking_are_not_kept() {
        for row in parse_all(CURRENT) {
            let kept = format!("{row:?}");
            for private in ["Austin", "Visa", "TBA000000000000", "Happy birthday", "David Okafor"] {
                assert!(!kept.contains(private), "{private} in {kept}");
            }
        }
    }

    #[test]
    fn an_older_layout_matches_through_aliases() {
        let old = "Order Date,Order ID,Title,Category,ASIN/ISBN,Purchase Price Per Unit,Quantity,Shipment Status,Item Total\n\
                   03/15/2019,112-0000002-0000002,The Example Band: Night Market (Vinyl),Music,B000000003,$24.50,1,Shipped,$26.52\n\
                   03/15/2019,112-0000002-0000002,,Music,,$1.00,1,Shipped,$1.00\n";
        let rows = parse_all(old);
        assert_eq!(rows.len(), 1, "a line naming no item is passed over");
        let r = &rows[0];
        assert_eq!(r.item_name, "The Example Band: Night Market (Vinyl)");
        assert_eq!(r.occurred_at.to_rfc3339(), "2019-03-15T12:00:00+00:00");
        assert_eq!((r.unit_price_cents, r.total_cents), (Some(2450), Some(2652)));
        assert_eq!(r.status.as_deref(), Some("Shipped"));
        assert_eq!(r.currency, None);
    }

    #[test]
    fn a_line_without_an_asin_keys_on_its_name() {
        let csv = "Order ID,Order Date,Product Name\n113-1,2024-01-01T00:00:00Z,Gift Card\n113-1,2024-01-01T00:00:00Z,Gift Card\n";
        let rows = parse_all(csv);
        assert!(rows[0].source_stream_id.starts_with("amazon:113-1:"));
        assert!(rows[0].source_stream_id.ends_with(":0"));
        assert!(rows[1].source_stream_id.ends_with(":1"));
        assert_ne!(rows[0].source_stream_id, rows[1].source_stream_id);
    }

    #[test]
    fn lines_keep_their_keys_when_a_download_lists_them_in_another_order() {
        let header = "Order ID,Order Date,Product Name,ASIN\n";
        let a = "114-1,2024-01-01T00:00:00Z,Example Desk Lamp,B000000004\n";
        let b = "114-1,2024-01-01T00:00:00Z,Paper Lanterns,B000000005\n";
        let keys = |csv: String| {
            let mut k: Vec<String> = parse_all(&csv).into_iter().map(|p| p.source_stream_id).collect();
            k.sort();
            k
        };
        let first = keys(format!("{header}{a}{b}{a}"));
        assert_eq!(first, keys(format!("{header}{b}{a}{a}")));
        assert_eq!(
            first,
            vec!["amazon:114-1:B000000004:0", "amazon:114-1:B000000004:1", "amazon:114-1:B000000005:0"]
        );
    }

    #[test]
    fn prices_parse_to_cents() {
        assert_eq!(cents("$1,299.00"), Some(129_900));
        assert_eq!(cents("12.99"), Some(1299));
        assert_eq!(cents("12.5"), Some(1250));
        assert_eq!(cents("12,99"), Some(1299));
        assert_eq!(cents("1.299,00"), Some(129_900));
        assert_eq!(cents("1,299"), Some(129_900));
        assert_eq!(cents("-$5.00"), Some(-500));
        assert_eq!(cents("0"), Some(0));
        assert_eq!(cents("Not Available"), None);
        assert_eq!(cents(""), None);
    }

    #[test]
    fn a_file_without_an_order_id_is_not_an_order_history() {
        let header: Vec<String> = ["Date", "Title"].iter().map(|s| s.to_string()).collect();
        assert!(Layout::from_header(&header).is_none());
        assert!(is_order_history_file("Retail.OrderHistory.1.csv"));
        assert!(is_order_history_file("Retail.OrderHistory.2.csv"));
        assert!(!is_order_history_file("Retail.OrderHistory.1.json"));
        assert!(!is_order_history_file("Retail.CustomerReturns.1.csv"));
    }
}
