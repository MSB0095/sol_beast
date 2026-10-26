//! Hardened HTTP API.
//!
//! Security properties:
//! * loopback bind by default (enforced by `beast-core` config validation);
//! * two distinct tokens, read-only and admin;
//! * CORS allowlist, never `*`;
//! * constant-time token comparison;
//! * policy-token request limiting;
//! * secrets and RPC keys are redacted from `/v1/config`.

use axum::extract::{Path, Query, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use beast_core::config::BeastConfig;
use beast_core::metrics::Metrics;
use beast_core::risk::RiskEngine;
use beast_core::types::{Mode, StrategyKind};
use beast_dex::catalog::{self, VENUES};
use beast_dex::jupiter::{JupiterClient, QuoteRequest};
use beast_dex::Address;
use beast_exec::RentReclaimer;
use beast_feed::PoolFeed;
use beast_store::Store;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::path::Path as FsPath;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::net::TcpListener;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("rate limited")]
    RateLimited,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("internal: {0}")]
    Internal(String),
    #[error("upstream: {0}")]
    Upstream(String),
    #[error("build: {0}")]
    Build(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) | Self::Build(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let message = match &self {
            Self::Internal(detail) | Self::Build(detail) => {
                tracing::error!(error = %detail, "API internal error");
                "internal server error".to_string()
            }
            Self::Upstream(detail) => {
                tracing::warn!(error = %detail, "API upstream error");
                "upstream error".to_string()
            }
            other => other.to_string(),
        };
        let mut response = (status, Json(json!({ "error": message }))).into_response();
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer realm=\"sol-beast\""),
            );
        }
        response
    }
}

impl From<beast_store::StoreError> for ApiError {
    fn from(error: beast_store::StoreError) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self::Internal(error.to_string())
    }
}

#[derive(Clone)]
pub struct ControlPlane {
    pub mode: Arc<tokio::sync::RwLock<Mode>>,
    pub strategies: Arc<tokio::sync::RwLock<HashMap<String, Mode>>>,
    pub kill: Arc<AtomicBool>,
}

impl ControlPlane {
    pub fn from_config(config: &BeastConfig) -> Self {
        let strategies = config
            .strategies
            .iter()
            .map(|(name, strategy)| (name.clone(), strategy.mode))
            .collect();
        Self {
            mode: Arc::new(tokio::sync::RwLock::new(config.runtime.mode)),
            strategies: Arc::new(tokio::sync::RwLock::new(strategies)),
            kill: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub struct ApiState {
    pub store: Arc<Store>,
    pub feed: Option<Arc<PoolFeed>>,
    pub metrics: Arc<Metrics>,
    pub config: Arc<BeastConfig>,
    pub mode: Arc<tokio::sync::RwLock<Mode>>,
    pub strategies: Arc<tokio::sync::RwLock<HashMap<String, Mode>>>,
    pub kill: Arc<AtomicBool>,
    pub risk: Option<Arc<Mutex<RiskEngine>>>,
    pub jupiter: Option<Arc<JupiterClient>>,
    pub rent: Option<Arc<RentReclaimer>>,
    /// True only when the process was started with `--real` *and*
    /// `runtime.real=true`. The API uses this for any signing endpoint.
    pub signing_enabled: bool,
    auth: Arc<ApiAuth>,
    read_gate: RateGate,
    admin_gate: RateGate,
    started_at: Instant,
}

impl ApiState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Arc<BeastConfig>,
        store: Arc<Store>,
        feed: Option<Arc<PoolFeed>>,
        metrics: Arc<Metrics>,
        risk: Option<Arc<Mutex<RiskEngine>>>,
        jupiter: Option<Arc<JupiterClient>>,
        admin_token: String,
        read_token: String,
    ) -> Result<Self> {
        let auth = Arc::new(ApiAuth::new(admin_token, read_token)?);
        let controls = ControlPlane::from_config(&config);
        Self::with_controls(config, store, feed, metrics, risk, jupiter, auth, controls)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_controls(
        config: Arc<BeastConfig>,
        store: Arc<Store>,
        feed: Option<Arc<PoolFeed>>,
        metrics: Arc<Metrics>,
        risk: Option<Arc<Mutex<RiskEngine>>>,
        jupiter: Option<Arc<JupiterClient>>,
        auth: Arc<ApiAuth>,
        controls: ControlPlane,
    ) -> Result<Self> {
        Ok(Self {
            store,
            feed,
            metrics,
            mode: controls.mode,
            strategies: controls.strategies,
            kill: controls.kill,
            risk,
            jupiter,
            rent: None,
            signing_enabled: false,
            auth,
            read_gate: RateGate::new(config.api.read_rate_limit_per_sec),
            admin_gate: RateGate::new(config.api.rate_limit_per_sec),
            config,
            started_at: Instant::now(),
        })
    }

    pub fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}

pub struct ApiAuth {
    admin: String,
    read: String,
}

impl ApiAuth {
    pub fn new(admin: String, read: String) -> Result<Self> {
        if admin.len() < 24 || read.len() < 24 {
            return Err(ApiError::Build(
                "API tokens must each be at least 24 characters".into(),
            ));
        }
        if admin == read {
            return Err(ApiError::Build(
                "admin and read-only API tokens must differ".into(),
            ));
        }
        Ok(Self { admin, read })
    }

    fn verify_admin(&self, token: &str) -> bool {
        constant_time_eq(self.admin.as_bytes(), token.as_bytes())
    }

    fn verify_read(&self, token: &str) -> bool {
        self.verify_admin(token) || constant_time_eq(self.read.as_bytes(), token.as_bytes())
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        difference |= (a ^ b) as usize;
    }
    difference == 0
}

struct RateGate {
    limit: u32,
    window: Mutex<RateWindow>,
}

struct RateWindow {
    started: Instant,
    count: u32,
}

impl RateGate {
    fn new(limit: u32) -> Self {
        Self {
            limit: limit.max(1),
            window: Mutex::new(RateWindow {
                started: Instant::now(),
                count: 0,
            }),
        }
    }

    fn allow(&self) -> bool {
        let Ok(mut window) = self.window.lock() else {
            return false;
        };
        let now = Instant::now();
        if now.duration_since(window.started) >= Duration::from_secs(1) {
            window.started = now;
            window.count = 0;
        }
        if window.count >= self.limit {
            return false;
        }
        window.count += 1;
        true
    }
}

pub fn build_router(state: Arc<ApiState>) -> Result<Router> {
    let mut cors = CorsLayer::new()
        .allow_methods(AllowMethods::list([
            Method::GET,
            Method::POST,
            Method::DELETE,
            Method::OPTIONS,
        ]))
        .allow_headers(AllowHeaders::list([
            AUTHORIZATION,
            CONTENT_TYPE,
            axum::http::header::HeaderName::from_static("x-api-token"),
        ]))
        .max_age(Duration::from_secs(600));
    let origins = &state.config.api.allowed_origins;
    if !origins.is_empty() {
        let mut values = Vec::with_capacity(origins.len());
        for origin in origins {
            if origin == "*" {
                return Err(ApiError::Build(
                    "api.allowed_origins must not contain a wildcard".into(),
                ));
            }
            values.push(HeaderValue::from_str(origin).map_err(|error| {
                ApiError::Build(format!("invalid CORS origin {origin:?}: {error}"))
            })?);
        }
        cors = cors.allow_origin(AllowOrigin::list(values));
    }

    let public = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        // Keep unknown API paths out of the SPA fallback.
        .route("/v1/*path", axum::routing::any(not_found));

    let read = Router::new()
        .route("/v1/status", get(status))
        .route("/v1/dashboard", get(dashboard))
        .route("/v1/ready", get(readyz))
        .route("/v1/healthz", get(healthz))
        .route("/v1/config", get(config))
        .route("/v1/venues", get(venues))
        .route("/v1/pools", get(pools))
        .route("/v1/opportunities", get(opportunities))
        .route("/v1/attempts", get(attempts))
        .route("/v1/bundles", get(bundles))
        .route("/v1/settlements", get(settlements))
        .route("/v1/report", get(report))
        .route("/v1/rent", get(rent_scan))
        .route("/v1/events", get(events))
        .route("/v1/strategies", get(strategy_list))
        .route("/v1/metrics", get(metrics))
        .route("/v1/quote", post(quote))
        .route_layer(middleware::from_fn_with_state(state.clone(), read_auth));

    let admin = Router::new()
        .route("/v1/mode", post(set_mode))
        .route("/v1/kill", post(kill))
        .route("/v1/resume", post(resume))
        .route("/v1/strategies/:name", post(set_strategy))
        .route("/v1/feed/refresh", post(refresh_feed))
        .route("/v1/pools", post(add_pool))
        .route("/v1/pools/:address", delete(remove_pool))
        .route("/v1/rent/reclaim", post(rent_reclaim))
        .route_layer(middleware::from_fn_with_state(state.clone(), admin_auth));

    let app = Router::new()
        .merge(public)
        .merge(read)
        .merge(admin)
        .layer(cors)
        .layer(RequestBodyLimitLayer::new(
            state.config.api.body_limit_bytes.max(1024),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    if let Some(directory) = &state.config.api.static_dir {
        let directory = directory.canonicalize().map_err(|error| {
            ApiError::Build(format!(
                "api.static_dir {:?} is not readable: {error}",
                directory
            ))
        })?;
        if !directory.is_dir() {
            return Err(ApiError::Build(format!(
                "api.static_dir {:?} is not a directory",
                directory
            )));
        }
        let index = directory.join("index.html");
        if !index.is_file() {
            return Err(ApiError::Build(format!(
                "api.static_dir is missing index.html: {:?}",
                index
            )));
        }
        Ok(app.fallback_service(ServeDir::new(&directory).fallback(ServeFile::new(index))))
    } else {
        Ok(app.fallback(not_found))
    }
}

pub async fn serve<F>(state: Arc<ApiState>, shutdown: F) -> std::io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let bind = state.config.api.bind.clone();
    let app = build_router(state).map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string())
    })?;
    let listener = TcpListener::bind(&bind).await?;
    tracing::info!(address = %listener.local_addr()?, "API listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
}

async fn read_auth(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if !state.read_gate.allow() {
        return ApiError::RateLimited.into_response();
    }
    if let Some(token) = token_from_headers(&headers) {
        if state.auth.verify_read(&token) {
            return next.run(request).await;
        }
    }
    ApiError::Unauthorized.into_response()
}

async fn admin_auth(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if !state.admin_gate.allow() {
        return ApiError::RateLimited.into_response();
    }
    if let Some(token) = token_from_headers(&headers) {
        if state.auth.verify_admin(&token) {
            return next.run(request).await;
        }
    }
    ApiError::Unauthorized.into_response()
}

fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get("x-api-token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(value.to_string());
    }
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = value.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") {
        Some(token.trim().to_string())
    } else {
        None
    }
}

async fn healthz(State(state): State<Arc<ApiState>>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "sol-beast-api",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": state.uptime_seconds(),
    }))
}

async fn readyz(State(state): State<Arc<ApiState>>) -> Response {
    let db_ok = state.store.quick_check().is_ok();
    let feed = match &state.feed {
        Some(feed) => feed.health().await,
        None => beast_feed::FeedHealth {
            pools: 0,
            quotable: 0,
            stale: 0,
            last_refresh_ms: None,
            age_ms: None,
            last_error: None,
        },
    };
    let stale_limit = state
        .feed
        .as_ref()
        .map(|feed| feed.config().stale_after_ms.saturating_mul(2))
        .unwrap_or(10_000);
    let feed_ok = feed.pools == 0 || feed.age_ms.unwrap_or(u64::MAX) <= stale_limit;
    let ready = db_ok && feed_ok;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({
            "ready": ready,
            "db": if db_ok { "ok" } else { "error" },
            "feed": feed,
        })),
    )
        .into_response()
}

async fn status(State(state): State<Arc<ApiState>>) -> Result<Json<Value>> {
    let stats = state.store.stats()?;
    let feed = match &state.feed {
        Some(feed) => Some(feed.health().await),
        None => None,
    };
    let risk = state
        .risk
        .as_ref()
        .and_then(|risk| risk.lock().ok().map(|risk| risk.snapshot()));
    Ok(Json(json!({
        "service": "sol-beast",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": state.uptime_seconds(),
        "mode": state.mode.read().await.as_str(),
        "configured_mode": state.config.runtime.mode.as_str(),
        "real": state.config.runtime.real,
        "signing_enabled": state.signing_enabled,
        "kill_switch": state.kill.load(Ordering::Relaxed),
        "feed": feed,
        "risk": risk,
        "store": stats,
        "metrics": state.metrics.snapshot(),
        "jupiter_configured": state.jupiter.is_some(),
        "rent_configured": state.rent.is_some(),
        "guard": {
            "enabled": state.config.guard.enabled,
            "mode": state.config.guard.mode,
            "program_id_configured": !state.config.guard.program_id.is_empty(),
            "min_profit_lamports": state.config.guard.min_profit_lamports,
            "unguarded_live_allowed": state.config.safety.allow_unguarded_live,
        },
        "jito_enabled": state.config.jito.enabled,
        "wallet": {
            "keypair_configured": keypair_present(&state.config),
        },
    })))
}

#[derive(Debug, Deserialize)]
struct DashboardQuery {
    #[serde(default)]
    limit: Option<usize>,
}

async fn dashboard(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<DashboardQuery>,
) -> Result<Json<Value>> {
    let limit = limit(query.limit, 60, 500);
    let stats = state.store.stats()?;
    let feed = match &state.feed {
        Some(feed) => Some(feed.health().await),
        None => None,
    };
    let risk = state
        .risk
        .as_ref()
        .and_then(|risk| risk.lock().ok().map(|risk| risk.snapshot()));
    let process_mode = *state.mode.read().await;
    let modes = state.strategies.read().await;
    let strategies: Vec<Value> = StrategyKind::ALL
        .iter()
        .map(|strategy| {
            let mode = modes
                .get(strategy.as_str())
                .copied()
                .unwrap_or(Mode::Off)
                .capped_by(process_mode);
            json!({
                "name": strategy.as_str(),
                "mode": mode.as_str(),
                "configured": state.config.strategies.contains_key(strategy.as_str()),
                "requires_bundle_guard": strategy.requires_bundle_guard(),
            })
        })
        .collect();
    Ok(Json(json!({
        "status": {
            "service": "sol-beast",
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_seconds": state.uptime_seconds(),
            "mode": process_mode.as_str(),
            "configured_mode": state.config.runtime.mode.as_str(),
            "real": state.config.runtime.real,
            "signing_enabled": state.signing_enabled,
            "kill_switch": state.kill.load(Ordering::Relaxed),
            "feed": feed,
            "risk": risk,
            "store": stats,
            "metrics": state.metrics.snapshot(),
            "jupiter_configured": state.jupiter.is_some(),
            "rent_configured": state.rent.is_some(),
            "guard": {
                "enabled": state.config.guard.enabled,
                "mode": state.config.guard.mode,
                "program_id_configured": !state.config.guard.program_id.is_empty(),
                "min_profit_lamports": state.config.guard.min_profit_lamports,
            },
            "jito_enabled": state.config.jito.enabled,
            "api": {
                "enabled": state.config.api.enabled,
                "bind": state.config.api.bind,
            },
            "wallet": {
                "keypair_configured": keypair_present(&state.config),
            },
        },
        "strategies": strategies,
        "venues": {
            "count": VENUES.len(),
            "coverage": coverage_summary(),
        },
        "opportunities": state.store.list_opportunities(limit)?,
        "attempts": state.store.list_attempts(limit)?,
        "bundles": state.store.list_bundles(limit)?,
        "settlements": state.store.list_settlements(limit)?,
        "events": state.store.recent_events(limit)?,
        "strategies_summary": state.store.strategy_summary()?,
    })))
}

async fn config(State(state): State<Arc<ApiState>>) -> Result<Json<Value>> {
    let mut value = serde_json::to_value(&*state.config)?;
    redact_config(&mut value);
    Ok(Json(value))
}

fn redact_config(value: &mut Value) {
    if let Some(urls) = value
        .get_mut("rpc")
        .and_then(|rpc| rpc.get_mut("urls"))
        .and_then(Value::as_array_mut)
    {
        for url in urls {
            if let Some(text) = url.as_str() {
                *url = Value::String(sanitize_url(text));
            }
        }
    }
    if let Some(urls) = value
        .get_mut("rpc")
        .and_then(|rpc| rpc.get_mut("ws_urls"))
        .and_then(Value::as_array_mut)
    {
        for url in urls {
            if let Some(text) = url.as_str() {
                *url = Value::String(sanitize_url(text));
            }
        }
    }
    for (section, keys) in [
        (
            "venues",
            &["jupiter_quote_url", "jupiter_swap_instructions_url"][..],
        ),
        ("jito", &["tip_floor_url"][..]),
    ] {
        if let Some(table) = value.get_mut(section).and_then(Value::as_object_mut) {
            for key in keys {
                if let Some(text) = table.get(*key).and_then(Value::as_str) {
                    table.insert((*key).to_string(), Value::String(sanitize_url(text)));
                }
            }
        }
    }
    if let Some(urls) = value
        .get_mut("jito")
        .and_then(|jito| jito.get_mut("block_engine_urls"))
        .and_then(Value::as_array_mut)
    {
        for url in urls {
            if let Some(text) = url.as_str() {
                *url = Value::String(sanitize_url(text));
            }
        }
    }
}

fn keypair_present(config: &BeastConfig) -> bool {
    config
        .wallet
        .keypair_path
        .as_ref()
        .is_some_and(|path| path.is_file())
}

fn sanitize_url(raw: &str) -> String {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return "[redacted]".into();
    };
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    if authority.is_empty() {
        format!("{scheme}://[redacted]")
    } else {
        format!("{scheme}://{authority}/[redacted]")
    }
}

async fn venues() -> Json<Value> {
    let list: Vec<Value> = VENUES
        .iter()
        .map(|venue| {
            json!({
                "id": venue.id,
                "name": venue.name,
                "program_id": venue.program_id,
                "kind": venue.kind,
                "coverage": venue.coverage.as_str(),
                "decoder": venue.decoder,
                "math": venue.math,
                "docs": venue.docs,
                "notes": venue.notes,
            })
        })
        .collect();
    Json(json!({
        "count": list.len(),
        "coverage": coverage_summary(),
        "venues": list,
    }))
}

fn coverage_summary() -> Value {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    for venue in VENUES {
        *counts.entry(venue.coverage.as_str()).or_default() += 1;
    }
    json!(counts)
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    source: Option<String>,
}

fn limit(value: Option<usize>, default: usize, max: usize) -> usize {
    value.unwrap_or(default).clamp(1, max)
}

async fn pools(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>> {
    let limit = limit(query.limit, 500, 5_000);
    let source = query.source.as_deref().unwrap_or("feed");
    let pools = if source == "db" {
        state.store.load_pools(limit)?
    } else if let Some(feed) = state.feed.as_ref() {
        feed.snapshots().await
    } else {
        state.store.load_pools(limit)?
    };
    let count = pools.len();
    Ok(Json(
        json!({ "count": count, "source": source, "pools": pools }),
    ))
}

async fn opportunities(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>> {
    let rows = state
        .store
        .list_opportunities(limit(query.limit, 100, 1_000))?;
    Ok(Json(json!({ "count": rows.len(), "opportunities": rows })))
}

async fn attempts(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>> {
    let rows = state.store.list_attempts(limit(query.limit, 100, 1_000))?;
    Ok(Json(json!({ "count": rows.len(), "attempts": rows })))
}

async fn bundles(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>> {
    let rows = state.store.list_bundles(limit(query.limit, 100, 1_000))?;
    Ok(Json(json!({ "count": rows.len(), "bundles": rows })))
}

async fn settlements(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>> {
    let rows = state
        .store
        .list_settlements(limit(query.limit, 100, 1_000))?;
    Ok(Json(json!({ "count": rows.len(), "settlements": rows })))
}

async fn report(State(state): State<Arc<ApiState>>) -> Result<Json<Value>> {
    Ok(Json(json!({
        "strategies": state.store.strategy_summary()?,
        "settlements": state.store.list_settlements(50)?,
        "stats": state.store.stats()?,
    })))
}

async fn rent_scan(State(state): State<Arc<ApiState>>) -> Result<Json<Value>> {
    let rent = state
        .rent
        .as_ref()
        .ok_or_else(|| ApiError::Conflict("rent reclaimer is not configured".into()))?;
    let scan = rent
        .scan()
        .await
        .map_err(|error| ApiError::Upstream(error.to_string()))?;
    Ok(Json(json!(scan)))
}

#[derive(Debug, Deserialize, Default)]
struct RentReclaimBody {
    #[serde(default)]
    confirm: bool,
}

async fn rent_reclaim(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<RentReclaimBody>,
) -> Result<Json<Value>> {
    let rent = state
        .rent
        .as_ref()
        .ok_or_else(|| ApiError::Conflict("rent reclaimer is not configured".into()))?;
    if state.kill.load(Ordering::Relaxed) {
        return Err(ApiError::Conflict(
            "kill switch is active; refusing to send rent transactions".into(),
        ));
    }
    if body.confirm && !state.signing_enabled {
        return Err(ApiError::Conflict(
            "rent reclaimer needs runtime.real=true and --real; use /v1/rent for a dry-run scan"
                .into(),
        ));
    }
    let real = body.confirm && state.signing_enabled;
    let report = rent
        .reclaim(real)
        .await
        .map_err(|error| ApiError::Upstream(error.to_string()))?;
    if !report.dry_run {
        state
            .metrics
            .rent_closed_accounts
            .fetch_add(report.closed as u64, Ordering::Relaxed);
        state
            .metrics
            .rent_recovered_lamports
            .fetch_add(report.net_recovered_lamports, Ordering::Relaxed);
    }
    state.store.record_event(
        if report.errors.is_empty() {
            "info"
        } else {
            "warn"
        },
        "rent",
        if report.dry_run {
            "rent dry-run prepared"
        } else {
            "rent reclaimer sent"
        },
        json!({
            "dry_run": report.dry_run,
            "closed": report.closed,
            "net_recovered_lamports": report.net_recovered_lamports,
            "errors": report.errors,
        }),
    )?;
    Ok(Json(json!(report)))
}

async fn events(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>> {
    let rows = state.store.recent_events(limit(query.limit, 100, 1_000))?;
    Ok(Json(json!({ "count": rows.len(), "events": rows })))
}

async fn strategy_list(State(state): State<Arc<ApiState>>) -> Json<Value> {
    let process_mode = *state.mode.read().await;
    let modes = state.strategies.read().await;
    let list: Vec<Value> = StrategyKind::ALL
        .iter()
        .map(|strategy| {
            let mode = modes
                .get(strategy.as_str())
                .copied()
                .unwrap_or(Mode::Off)
                .capped_by(process_mode);
            json!({
                "name": strategy.as_str(),
                "mode": mode.as_str(),
                "configured": state.config.strategies.contains_key(strategy.as_str()),
                "requires_bundle_guard": strategy.requires_bundle_guard(),
            })
        })
        .collect();
    Json(json!({ "strategies": list }))
}

async fn metrics(State(state): State<Arc<ApiState>>) -> Response {
    let text = state.metrics.prometheus();
    (
        [(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        text,
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
struct QuoteBody {
    input_mint: String,
    output_mint: String,
    amount: u64,
    #[serde(default)]
    slippage_bps: Option<u16>,
}

async fn quote(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<QuoteBody>,
) -> Result<Json<Value>> {
    let jupiter = state
        .jupiter
        .as_ref()
        .ok_or_else(|| ApiError::Conflict("Jupiter is not configured".into()))?;
    let input = body
        .input_mint
        .parse::<Address>()
        .map_err(|error| ApiError::BadRequest(format!("input_mint: {error}")))?;
    let output = body
        .output_mint
        .parse::<Address>()
        .map_err(|error| ApiError::BadRequest(format!("output_mint: {error}")))?;
    if body.amount == 0 {
        return Err(ApiError::BadRequest("amount must be > 0".into()));
    }
    let mut request = QuoteRequest::new(input, output, body.amount);
    request.slippage_bps = body.slippage_bps.unwrap_or(50).clamp(1, 500);
    let result = jupiter
        .quote(&request)
        .await
        .map_err(|error| ApiError::Upstream(error.to_string()))?;
    Ok(Json(json!(result)))
}

#[derive(Debug, Deserialize)]
struct ModeBody {
    mode: String,
}

async fn set_mode(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<ModeBody>,
) -> Result<Json<Value>> {
    let requested =
        Mode::from_str(&body.mode).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    if requested.rank() > state.config.runtime.mode.rank() {
        return Err(ApiError::Conflict(format!(
            "requested mode {} exceeds configured ceiling {}",
            requested, state.config.runtime.mode
        )));
    }
    if requested.is_live() && state.kill.load(Ordering::Relaxed) {
        return Err(ApiError::Conflict(
            "kill switch is active; resume before enabling live mode".into(),
        ));
    }
    *state.mode.write().await = requested;
    state.store.set_meta("mode", requested.as_str())?;
    state.store.record_event(
        "info",
        "api",
        "mode changed",
        json!({"mode": requested.as_str()}),
    )?;
    Ok(Json(json!({"mode": requested.as_str()})))
}

#[derive(Debug, Deserialize, Default)]
struct KillBody {
    #[serde(default)]
    reason: Option<String>,
}

async fn kill(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<KillBody>,
) -> Result<Json<Value>> {
    let reason = body
        .reason
        .unwrap_or_else(|| "manual kill switch via API".to_string());
    state.kill.store(true, Ordering::SeqCst);
    if let Some(risk) = &state.risk {
        if let Ok(mut risk) = risk.lock() {
            risk.halt("api_kill", reason.clone());
        }
    }
    let path = state.config.risk.kill_switch_file.clone();
    write_kill_file(&path, &reason)?;
    state.store.record_event(
        "warn",
        "api",
        "kill switch activated",
        json!({"reason": reason, "file": path}),
    )?;
    Ok(Json(
        json!({"killed": true, "reason": reason, "file": path}),
    ))
}

async fn resume(State(state): State<Arc<ApiState>>) -> Result<Json<Value>> {
    state.kill.store(false, Ordering::SeqCst);
    if let Some(risk) = &state.risk {
        if let Ok(mut risk) = risk.lock() {
            risk.resume();
        }
    }
    let path = state.config.risk.kill_switch_file.clone();
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(ApiError::Internal(format!(
                "cannot remove kill file: {error}"
            )))
        }
    }
    state
        .store
        .record_event("info", "api", "kill switch cleared", json!({"file": path}))?;
    Ok(Json(json!({"killed": false})))
}

fn write_kill_file(path: &FsPath, reason: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| ApiError::Internal(format!("cannot create kill dir: {error}")))?;
        }
    }
    std::fs::write(path, format!("{reason}\n"))
        .map_err(|error| ApiError::Internal(format!("cannot write kill file: {error}")))
}

#[derive(Debug, Deserialize)]
struct StrategyBody {
    mode: String,
}

async fn set_strategy(
    State(state): State<Arc<ApiState>>,
    Path(name): Path<String>,
    Json(body): Json<StrategyBody>,
) -> Result<Json<Value>> {
    let strategy =
        StrategyKind::from_str(&name).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let requested =
        Mode::from_str(&body.mode).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    if requested.rank() > state.config.runtime.mode.rank() {
        return Err(ApiError::Conflict(format!(
            "requested mode {} exceeds configured ceiling {}",
            requested, state.config.runtime.mode
        )));
    }
    state
        .strategies
        .write()
        .await
        .insert(strategy.as_str().to_string(), requested);
    state.store.set_meta(
        &format!("strategy:{}:mode", strategy.as_str()),
        requested.as_str(),
    )?;
    state.store.record_event(
        "info",
        "api",
        "strategy mode changed",
        json!({"strategy": strategy.as_str(), "mode": requested.as_str()}),
    )?;
    Ok(Json(json!({
        "strategy": strategy.as_str(),
        "mode": requested.as_str(),
    })))
}

async fn refresh_feed(State(state): State<Arc<ApiState>>) -> Result<Json<Value>> {
    let feed = state
        .feed
        .as_ref()
        .ok_or_else(|| ApiError::Conflict("feed is not configured".into()))?;
    let report = feed.refresh_all().await;
    state.store.record_event(
        "info",
        "api",
        "manual feed refresh",
        json!({"attempted": report.attempted, "updated": report.updated, "failed": report.failed}),
    )?;
    Ok(Json(json!(report)))
}

#[derive(Debug, Deserialize)]
struct AddPoolBody {
    address: String,
    venue: String,
    #[serde(default)]
    decimals_a: Option<u8>,
    #[serde(default)]
    decimals_b: Option<u8>,
    #[serde(default)]
    fee_bps: Option<u32>,
}

async fn add_pool(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<AddPoolBody>,
) -> Result<Json<Value>> {
    let feed = state
        .feed
        .as_ref()
        .ok_or_else(|| ApiError::Conflict("feed is not configured".into()))?;
    let address = body
        .address
        .parse::<Address>()
        .map_err(|error| ApiError::BadRequest(format!("address: {error}")))?;
    let venue = catalog::venue(&body.venue)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown venue {}", body.venue)))?;
    if venue.decoder.is_none() {
        return Err(ApiError::BadRequest(format!(
            "venue {} is quote-only and has no local account decoder",
            body.venue
        )));
    }
    let mut watched = beast_feed::WatchedPool::from_parts(address, body.venue.clone());
    if let (Some(a), Some(b)) = (body.decimals_a, body.decimals_b) {
        watched = watched.with_decimals(a, b);
    }
    if let Some(fee_bps) = body.fee_bps {
        watched = watched.with_fee_override(fee_bps);
    }
    feed.add(watched.clone()).await;
    state.store.record_event(
        "info",
        "api",
        "pool added to feed",
        json!({"address": address, "venue": body.venue}),
    )?;
    Ok(Json(json!({"added": true, "pool": watched})))
}

async fn remove_pool(
    State(state): State<Arc<ApiState>>,
    Path(address): Path<String>,
) -> Result<Json<Value>> {
    let feed = state
        .feed
        .as_ref()
        .ok_or_else(|| ApiError::Conflict("feed is not configured".into()))?;
    let address = address
        .parse::<Address>()
        .map_err(|error| ApiError::BadRequest(format!("address: {error}")))?;
    let removed = feed.remove(address).await;
    if !removed {
        return Err(ApiError::NotFound);
    }
    let _ = state.store.delete_pool(address)?;
    state.store.record_event(
        "info",
        "api",
        "pool removed from feed",
        json!({"address": address}),
    )?;
    Ok(Json(json!({"removed": true})))
}

async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Json(json!({"error": "not found"})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state() -> (Arc<ApiState>, Router) {
        let mut config = BeastConfig::default();
        config.api.read_rate_limit_per_sec = 10_000;
        config.api.rate_limit_per_sec = 10_000;
        config.guard.program_id = String::new();
        let config = Arc::new(config);
        let store = Arc::new(Store::in_memory().unwrap());
        let state = Arc::new(
            ApiState::new(
                config,
                store,
                None,
                Arc::new(Metrics::new()),
                None,
                None,
                "beast_admin_0123456789abcdef0123456789".into(),
                "beast_read_0123456789abcdef0123456789".into(),
            )
            .unwrap(),
        );
        let router = build_router(state.clone()).unwrap();
        (state, router)
    }

    async fn body_json(response: Response) -> Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn health_is_public() {
        let (_state, router) = test_state();
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn read_routes_require_token() {
        let (_state, router) = test_state();
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/status")
                    .header(
                        AUTHORIZATION,
                        "Bearer beast_read_0123456789abcdef0123456789",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn admin_routes_reject_read_token() {
        let (_state, router) = test_state();
        let body = Body::from(r#"{"mode":"radar"}"#);
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/mode")
                    .header("content-type", "application/json")
                    .header(
                        AUTHORIZATION,
                        "Bearer beast_read_0123456789abcdef0123456789",
                    )
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = Body::from(r#"{"mode":"radar"}"#);
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/mode")
                    .header("content-type", "application/json")
                    .header(
                        AUTHORIZATION,
                        "Bearer beast_admin_0123456789abcdef0123456789",
                    )
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn config_redacts_rpc_credentials() {
        let (state, _router) = test_state();
        let mut value = serde_json::to_value(&*state.config).unwrap();
        value["rpc"]["urls"] = json!(["https://user:pass@example.invalid/?api-key=secret"]);
        redact_config(&mut value);
        let urls = value["rpc"]["urls"].as_array().unwrap();
        assert_eq!(urls[0], "https://example.invalid/[redacted]");
        assert!(!urls[0].as_str().unwrap().contains("secret"));
    }

    #[tokio::test]
    async fn venues_lists_every_registered_venue() {
        let (_state, router) = test_state();
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/venues")
                    .header(
                        AUTHORIZATION,
                        "Bearer beast_read_0123456789abcdef0123456789",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["count"], 60);
    }
}
