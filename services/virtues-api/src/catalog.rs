//! The model catalog — fetched, never mirrored.
//!
//! Vercel AI Gateway publishes its full catalog at `GET /v1/models`: every
//! model that exists, with pricing, context window, and modality. It is
//! unauthenticated and CDN-cached. We fetch it on boot and hourly, and that is
//! the ONLY place model facts come from.
//!
//! # Why this exists
//!
//! We used to keep a hand-written copy of this data in `virtues-registry`.
//! Every entry in it had drifted:
//!
//!   claude-opus-4.8      we said $15/$75 per M   really $5/$25      (3× over)
//!   glm-4.7-flash        we said $0.30/$1.00     really $0.07/$0.40 (4× over)
//!   gemini-3-pro-image   we had no entry at all  really $2/$12      (13× UNDER)
//!   google/gemini-3-pro  in our picker           does not exist     (404s)
//!   openai/gpt-5.1       in our picker           never existed      (404s)
//!
//! The two phantoms are the important ones: Vercel publishes **no deprecation
//! notice and no sunset field** — a model id can simply stop working (Cohere's
//! Command R/R+ vanished exactly this way). Nothing checked, so nothing knew.
//!
//! `store()` now enforces `curated ⊆ catalog` on every refresh: a curated model
//! that disappears from the gateway is logged loudly and dropped from what we
//! serve, so it leaves the picker within the hour instead of 404ing a user
//! mid-chat.
//!
//! # Cadence
//!
//! Hourly. Vercel documents no rate limit and no recommended cadence, but their
//! own AI SDK defaults to re-fetching this every **5 minutes**
//! (`metadataCacheRefreshMillis`), so hourly is 12× more conservative than
//! their own client — and it costs one unauthenticated GET per hour from one
//! process, not per box.
//!
//! # Staleness
//!
//! A failed refresh keeps the last known-good snapshot. A gateway blip must not
//! empty the picker or drop billing to the fallback floor. Only a cold start
//! with no successful fetch leaves us empty, and that is the one case
//! `FALLBACK_PRICING` exists for.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::Deserialize;

/// Vercel's own SDK default is 5 minutes; hourly is deliberately conservative
/// and matches the `sweeper` interval.
const REFRESH_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// One model as the gateway describes it. We deserialize only what we use — the
/// gateway sends far more (tiered pricing, cache read/write rates, per-image
/// and per-second rates for other modalities) and adds more over time, which
/// must never break the parse.
#[derive(Debug, Clone, Deserialize)]
pub struct GatewayModel {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub owned_by: Option<String>,
    #[serde(default)]
    pub context_window: Option<i64>,
    #[serde(default)]
    pub max_tokens: Option<i64>,
    /// `language` | `embedding` | `image` | `reranking` | `speech` | …
    #[serde(default, rename = "type")]
    pub model_type: Option<String>,
    #[serde(default)]
    pub pricing: Option<GatewayPricing>,
}

/// Gateway pricing is **per token**, and arrives as **strings** (e.g.
/// `"0.00000012"`) to dodge float-precision games. Everything downstream speaks
/// per-1K, so `per_1k()` is the single conversion point.
#[derive(Debug, Clone, Deserialize)]
pub struct GatewayPricing {
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
}

impl GatewayPricing {
    /// `(input_cost_per_1k, output_cost_per_1k)` USD, or None if the gateway
    /// gave us nothing usable — in which case the caller must fall back. Never
    /// assume zero; that's a free-money bug.
    fn per_1k(&self) -> Option<(f64, f64)> {
        let input: f64 = self.input.as_ref()?.parse().ok()?;
        let output: f64 = self.output.as_ref()?.parse().ok()?;
        Some((input * 1000.0, output * 1000.0))
    }
}

#[derive(Debug, Deserialize)]
struct CatalogResponse {
    data: Vec<GatewayModel>,
}

/// Live model facts, shared across the process. Cheap to clone; reads are an
/// uncontended RwLock read and happen on the billing path.
#[derive(Clone, Default)]
pub struct Catalog {
    inner: Arc<RwLock<HashMap<String, GatewayModel>>>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pricing as `(input_per_1k, output_per_1k)`, or None when the catalog is
    /// cold or the model is unknown. Callers fall back to
    /// `virtues_registry::models::FALLBACK_PRICING`.
    pub fn pricing(&self, model_id: &str) -> Option<(f64, f64)> {
        let guard = self.inner.read().ok()?;
        guard.get(model_id)?.pricing.as_ref()?.per_1k()
    }

    pub fn get(&self, model_id: &str) -> Option<GatewayModel> {
        self.inner.read().ok()?.get(model_id).cloned()
    }

    pub fn is_cold(&self) -> bool {
        self.inner.read().map(|g| g.is_empty()).unwrap_or(true)
    }

    /// The curated picker, hydrated with live facts, minus anything the gateway
    /// no longer carries. This is `curated ∩ catalog` — the intersection is
    /// load-bearing, not paranoid; see the module docs.
    ///
    /// A cold catalog yields the curated list unhydrated rather than an empty
    /// one: a box that can't reach us should still show a working picker.
    pub fn curated(&self) -> Vec<CuratedModel> {
        let cold = self.is_cold();
        virtues_registry::models::default_models()
            .into_iter()
            .filter(|m| m.enabled)
            .filter(|m| cold || self.get(&m.model_id).is_some())
            .map(|m| {
                let live = self.get(&m.model_id);
                let pricing = self.pricing(&m.model_id);
                CuratedModel {
                    // Live facts win where we have them.
                    context_window: live
                        .as_ref()
                        .and_then(|l| l.context_window)
                        .unwrap_or(m.context_window as i64),
                    max_output_tokens: live
                        .as_ref()
                        .and_then(|l| l.max_tokens)
                        .unwrap_or(m.max_output_tokens as i64),
                    input_cost_per_1k: pricing.map(|p| p.0),
                    output_cost_per_1k: pricing.map(|p| p.1),
                    // Taste stays ours — especially the capability flags. The
                    // gateway's `tags` say what a model can do, not what works
                    // through its OpenAI-compatible shim (Gemini 3 advertises
                    // tool use and 400s on parallel calls).
                    model_id: m.model_id,
                    display_name: m.display_name,
                    provider: m.provider,
                    sort_order: m.sort_order,
                    supports_tools: m.supports_tools,
                    supports_vision: m.supports_vision,
                    supports_pdf: m.supports_pdf,
                    supports_audio: m.supports_audio,
                    is_default: m.is_default,
                }
            })
            .collect()
    }

    /// Replace the snapshot and enforce `curated ⊆ catalog`.
    fn store(&self, models: Vec<GatewayModel>) {
        let map: HashMap<String, GatewayModel> =
            models.into_iter().map(|m| (m.id.clone(), m)).collect();

        // The only deprecation warning we will ever get.
        let missing: Vec<String> = virtues_registry::models::required_model_ids()
            .into_iter()
            .filter(|id| !map.contains_key(id))
            .collect();
        if !missing.is_empty() {
            tracing::error!(
                missing = ?missing,
                "curated models are NOT in the gateway catalog — they will 404. \
                 Dropped from the picker; fix crates/virtues-registry/src/models.rs"
            );
        }

        if let Ok(mut guard) = self.inner.write() {
            *guard = map;
        }
    }
}

/// Fetch the catalog once. Unauthenticated by design — Vercel's docs are
/// explicit that `/v1/models` "requires no authentication" — so this works even
/// before a gateway key is configured.
pub async fn fetch(http: &reqwest::Client, gateway_url: &str) -> anyhow::Result<Vec<GatewayModel>> {
    let url = format!("{gateway_url}/v1/models");
    let resp = http.get(&url).send().await?.error_for_status()?;
    Ok(resp.json::<CatalogResponse>().await?.data)
}

/// Boot-time fetch + hourly refresh. Returns the shared handle immediately.
///
/// The first fetch is awaited so a healthy start never bills against a cold
/// catalog — but a *failed* first fetch is not fatal. We log and serve from the
/// fallback floor until a later refresh succeeds. AI must not be unavailable
/// because a metadata endpoint had a bad minute.
pub async fn spawn(http: reqwest::Client, gateway_url: String) -> Catalog {
    let catalog = Catalog::new();

    match fetch(&http, &gateway_url).await {
        Ok(models) => {
            tracing::info!(count = models.len(), "model catalog loaded");
            catalog.store(models);
        }
        Err(e) => tracing::error!(
            "initial model catalog fetch failed: {e:#} — \
             billing falls back to FALLBACK_PRICING until a refresh succeeds"
        ),
    }

    let bg = catalog.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(REFRESH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await; // the immediate first tick — we just fetched
        loop {
            interval.tick().await;
            match fetch(&http, &gateway_url).await {
                Ok(models) => {
                    tracing::debug!(count = models.len(), "model catalog refreshed");
                    bg.store(models);
                }
                // Keep the last known-good snapshot: a blip must not empty the
                // picker or drop billing to the floor.
                Err(e) => tracing::warn!("model catalog refresh failed: {e:#} — keeping stale"),
            }
        }
    });

    catalog
}

/// The curated picker as served to boxes: our taste, the gateway's facts.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CuratedModel {
    pub model_id: String,
    pub display_name: String,
    pub provider: String,
    pub sort_order: i32,
    pub context_window: i64,
    pub max_output_tokens: i64,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_pdf: bool,
    pub supports_audio: bool,
    pub is_default: bool,
    /// From the live catalog. `None` only when the catalog is cold.
    pub input_cost_per_1k: Option<f64>,
    pub output_cost_per_1k: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pricing_converts_per_token_strings_to_per_1k() {
        let p = GatewayPricing {
            input: Some("0.00000007".to_string()), // $0.07 / M  (glm-4.7-flash)
            output: Some("0.0000004".to_string()), // $0.40 / M
        };
        let (i, o) = p.per_1k().unwrap();
        assert!((i - 0.00007).abs() < 1e-12, "input per-1k: {i}");
        assert!((o - 0.0004).abs() < 1e-12, "output per-1k: {o}");
    }

    #[test]
    fn missing_pricing_is_none_not_zero() {
        // Zero here would be a free-money bug: every call bills $0.00.
        let p = GatewayPricing {
            input: None,
            output: None,
        };
        assert!(p.per_1k().is_none());
    }

    #[test]
    fn cold_catalog_still_serves_the_picker() {
        let c = Catalog::new();
        assert!(c.is_cold());
        assert!(
            !c.curated().is_empty(),
            "a box that can't reach the gateway must still get a usable picker"
        );
        assert!(c.pricing("anthropic/claude-opus-4.8").is_none());
    }

    /// `curated ⊆ catalog` against the LIVE gateway. This is the check that was
    /// missing while `google/gemini-3-pro` and `openai/gpt-5.1` rotted in the
    /// picker — Vercel gives no deprecation notice, so polling is the only
    /// warning available.
    ///
    /// Networked, so ignored by default. CI runs `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "network: hits the live Vercel AI Gateway catalog"]
    async fn curated_models_all_exist_on_the_gateway() {
        // `main()` installs this process-wide; tests don't run `main()`.
        let _ = rustls::crypto::ring::default_provider().install_default();

        let http = reqwest::Client::new();
        let models = fetch(&http, "https://ai-gateway.vercel.sh")
            .await
            .expect("fetch gateway catalog");
        let live: std::collections::HashSet<String> = models.into_iter().map(|m| m.id).collect();

        let missing: Vec<String> = virtues_registry::models::required_model_ids()
            .into_iter()
            .filter(|id| !live.contains(id))
            .collect();

        assert!(
            missing.is_empty(),
            "these curated model ids do not exist on the gateway and will 404: {missing:?}"
        );
    }
}
