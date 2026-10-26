//! SQLite persistence for solver state.
//!
//! The store is deliberately boring: one WAL database, short transactions and
//! append-only history for opportunities, attempts and bundles. API reads and
//! the solver write path share the same connection behind a mutex, which is
//! fine at the rates this engine operates (tens of writes per second).

use beast_core::types::{
    CostBreakdown, LegPlan, Mode, Opportunity, OutcomeLabel, SettlementResult, StrategyKind,
};
use beast_dex::model::PoolSnapshot;
use beast_dex::Address;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("store lock poisoned")]
    Poisoned,
    #[error("invalid stored value: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityRecord {
    pub id: String,
    pub strategy: String,
    pub mode: String,
    pub slot_detected: u64,
    pub detected_at_ms: i64,
    pub start_mint: String,
    pub amount_in: u64,
    pub amount_out: u64,
    pub est_gross_lamports: i64,
    pub est_net_lamports: i64,
    pub confidence: f32,
    pub ttl_slots: u8,
    pub dedupe_key: String,
    pub outcome: String,
    pub legs_json: String,
    pub costs_json: String,
    pub meta_json: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AttemptUpdate {
    pub status: Option<String>,
    pub bundle_id: Option<String>,
    pub signature: Option<String>,
    pub error: Option<String>,
    pub realized_net_lamports: Option<i64>,
    pub landed: Option<bool>,
    pub reverted: Option<bool>,
    pub tip_lamports: Option<u64>,
    pub fee_lamports: Option<u64>,
    pub simulated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptRecord {
    pub id: String,
    pub opportunity_id: Option<String>,
    pub ts_ms: i64,
    pub strategy: String,
    pub mode: String,
    pub status: String,
    pub bundle_id: Option<String>,
    pub simulated: bool,
    pub landed: bool,
    pub reverted: bool,
    pub realized_net_lamports: Option<i64>,
    pub tip_lamports: u64,
    pub fee_lamports: u64,
    pub signature: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleRecord {
    pub bundle_id: String,
    pub attempt_id: Option<String>,
    pub ts_ms: i64,
    pub status: String,
    pub tip_lamports: u64,
    pub signatures_json: String,
    pub landed_slot: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: i64,
    pub ts_ms: i64,
    pub level: String,
    pub component: String,
    pub message: String,
    pub data_json: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoreStats {
    pub pools: i64,
    pub opportunities: i64,
    pub attempts: i64,
    pub landed: i64,
    pub reverted: i64,
    pub bundles: i64,
    pub events: i64,
    pub settlements: i64,
    pub realized_net_lamports: i64,
    pub last_event_ms: Option<i64>,
}

pub struct Store {
    path: PathBuf,
    conn: Mutex<Connection>,
}

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS pools (
    address TEXT PRIMARY KEY,
    venue TEXT NOT NULL,
    program_id TEXT NOT NULL,
    mint_a TEXT NOT NULL,
    mint_b TEXT NOT NULL,
    vault_a TEXT NOT NULL,
    vault_b TEXT NOT NULL,
    decimals_a INTEGER NOT NULL,
    decimals_b INTEGER NOT NULL,
    fee_bps INTEGER NOT NULL,
    model_json TEXT NOT NULL,
    slot INTEGER NOT NULL,
    confidence REAL NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pools_venue ON pools(venue);
CREATE INDEX IF NOT EXISTS idx_pools_mint_a ON pools(mint_a);
CREATE INDEX IF NOT EXISTS idx_pools_mint_b ON pools(mint_b);

CREATE TABLE IF NOT EXISTS opportunities (
    id TEXT PRIMARY KEY,
    strategy TEXT NOT NULL,
    mode TEXT NOT NULL,
    slot_detected INTEGER NOT NULL,
    detected_at_ms INTEGER NOT NULL,
    start_mint TEXT NOT NULL,
    amount_in INTEGER NOT NULL,
    amount_out INTEGER NOT NULL,
    est_gross_lamports INTEGER NOT NULL,
    est_net_lamports INTEGER NOT NULL,
    confidence REAL NOT NULL,
    ttl_slots INTEGER NOT NULL,
    dedupe_key TEXT NOT NULL,
    outcome TEXT NOT NULL,
    legs_json TEXT NOT NULL,
    borrow_json TEXT NOT NULL,
    costs_json TEXT NOT NULL,
    meta_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_opps_time ON opportunities(detected_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_opps_net ON opportunities(est_net_lamports DESC);
CREATE INDEX IF NOT EXISTS idx_opps_dedupe ON opportunities(dedupe_key);

CREATE TABLE IF NOT EXISTS attempts (
    id TEXT PRIMARY KEY,
    opportunity_id TEXT,
    ts_ms INTEGER NOT NULL,
    strategy TEXT NOT NULL,
    mode TEXT NOT NULL,
    status TEXT NOT NULL,
    bundle_id TEXT,
    simulated INTEGER NOT NULL DEFAULT 0,
    landed INTEGER NOT NULL DEFAULT 0,
    reverted INTEGER NOT NULL DEFAULT 0,
    realized_net_lamports INTEGER,
    tip_lamports INTEGER NOT NULL DEFAULT 0,
    fee_lamports INTEGER NOT NULL DEFAULT 0,
    signature TEXT,
    error TEXT
);
CREATE INDEX IF NOT EXISTS idx_attempts_time ON attempts(ts_ms DESC);
CREATE INDEX IF NOT EXISTS idx_attempts_status ON attempts(status);

CREATE TABLE IF NOT EXISTS bundles (
    bundle_id TEXT PRIMARY KEY,
    attempt_id TEXT,
    ts_ms INTEGER NOT NULL,
    status TEXT NOT NULL,
    tip_lamports INTEGER NOT NULL,
    signatures_json TEXT NOT NULL,
    landed_slot INTEGER,
    error TEXT
);
CREATE INDEX IF NOT EXISTS idx_bundles_time ON bundles(ts_ms DESC);

CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts_ms INTEGER NOT NULL,
    level TEXT NOT NULL,
    component TEXT NOT NULL,
    message TEXT NOT NULL,
    data_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_events_time ON events(ts_ms DESC);
CREATE INDEX IF NOT EXISTS idx_events_component ON events(component);

CREATE TABLE IF NOT EXISTS settlements (
    opportunity_id TEXT PRIMARY KEY,
    signature TEXT NOT NULL,
    slot INTEGER NOT NULL,
    realized_gross_lamports INTEGER NOT NULL,
    realized_tip_lamports INTEGER NOT NULL,
    realized_fee_lamports INTEGER NOT NULL,
    realized_net_lamports INTEGER NOT NULL,
    modeled_net_lamports INTEGER NOT NULL,
    settled_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(&path)?;
        Self::configure(&conn)?;
        let store = Self {
            path,
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        store.harden_permissions();
        Ok(store)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        let store = Self {
            path: PathBuf::from(":memory:"),
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    #[cfg(unix)]
    fn harden_permissions(&self) {
        use std::os::unix::fs::PermissionsExt;
        let base = self.path.to_string_lossy();
        for path in [
            self.path.clone(),
            PathBuf::from(format!("{base}-wal")),
            PathBuf::from(format!("{base}-shm")),
        ] {
            if let Ok(metadata) = std::fs::metadata(&path) {
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o600);
                let _ = std::fs::set_permissions(&path, permissions);
            }
        }
    }

    #[cfg(not(unix))]
    fn harden_permissions(&self) {}

    fn configure(conn: &Connection) -> Result<()> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        Ok(())
    }

    fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| StoreError::Poisoned)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn()?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            conn.execute_batch(SCHEMA_V1)?;
            conn.pragma_update(None, "user_version", 1)?;
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn upsert_pool(&self, pool: &PoolSnapshot) -> Result<()> {
        let model_json = serde_json::to_string(&pool.model)?;
        let now = chrono::Utc::now().timestamp_millis();
        self.conn()?.execute(
            r#"
            INSERT INTO pools (
                address, venue, program_id, mint_a, mint_b, vault_a, vault_b,
                decimals_a, decimals_b, fee_bps, model_json, slot, confidence, updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(address) DO UPDATE SET
                venue=excluded.venue,
                program_id=excluded.program_id,
                mint_a=excluded.mint_a,
                mint_b=excluded.mint_b,
                vault_a=excluded.vault_a,
                vault_b=excluded.vault_b,
                decimals_a=excluded.decimals_a,
                decimals_b=excluded.decimals_b,
                fee_bps=excluded.fee_bps,
                model_json=excluded.model_json,
                slot=excluded.slot,
                confidence=excluded.confidence,
                updated_at_ms=excluded.updated_at_ms
            "#,
            params![
                pool.address.to_string(),
                pool.venue,
                pool.program_id.to_string(),
                pool.mint_a.to_string(),
                pool.mint_b.to_string(),
                pool.vault_a.to_string(),
                pool.vault_b.to_string(),
                pool.decimals_a as i64,
                pool.decimals_b as i64,
                pool.fee_bps as i64,
                model_json,
                pool.slot as i64,
                pool.confidence as f64,
                now,
            ],
        )?;
        Ok(())
    }

    pub fn load_pools(&self, limit: usize) -> Result<Vec<PoolSnapshot>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            r#"
            SELECT address, venue, program_id, mint_a, mint_b, vault_a, vault_b,
                   decimals_a, decimals_b, fee_bps, model_json, slot, confidence
            FROM pools
            ORDER BY updated_at_ms DESC
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit as i64], row_to_pool)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn get_pool(&self, address: Address) -> Result<Option<PoolSnapshot>> {
        self.conn()?
            .query_row(
                r#"
                SELECT address, venue, program_id, mint_a, mint_b, vault_a, vault_b,
                       decimals_a, decimals_b, fee_bps, model_json, slot, confidence
                FROM pools WHERE address = ?1
                "#,
                params![address.to_string()],
                row_to_pool,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn delete_pool(&self, address: Address) -> Result<bool> {
        Ok(self.conn()?.execute(
            "DELETE FROM pools WHERE address = ?1",
            params![address.to_string()],
        )? > 0)
    }

    pub fn insert_opportunity(&self, opportunity: &Opportunity) -> Result<()> {
        let start_mint = opportunity
            .legs
            .first()
            .map(|leg| leg.mint_in.clone())
            .unwrap_or_default();
        let amount_in = opportunity
            .legs
            .first()
            .map(|leg| leg.amount_in)
            .unwrap_or(0);
        let amount_out = opportunity.legs.last().map(|leg| leg.min_out).unwrap_or(0);
        let legs_json = serde_json::to_string(&opportunity.legs)?;
        let borrow_json = "null";
        let costs_json = serde_json::to_string(&opportunity.est_costs)?;
        let meta_json = serde_json::to_string(&opportunity.meta)?;
        self.conn()?.execute(
            r#"
            INSERT OR REPLACE INTO opportunities (
                id, strategy, mode, slot_detected, detected_at_ms, start_mint,
                amount_in, amount_out, est_gross_lamports, est_net_lamports,
                confidence, ttl_slots, dedupe_key, outcome, legs_json, borrow_json,
                costs_json, meta_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
            "#,
            params![
                opportunity.id.to_string(),
                opportunity.strategy.as_str(),
                opportunity.mode.as_str(),
                opportunity.slot_detected as i64,
                opportunity.detected_at_ms,
                start_mint,
                amount_in as i64,
                amount_out as i64,
                opportunity.est_gross_lamports,
                opportunity.est_net_lamports,
                opportunity.confidence as f64,
                opportunity.ttl_slots as i64,
                opportunity.dedupe_key,
                opportunity.outcome.as_str(),
                legs_json,
                borrow_json,
                costs_json,
                meta_json,
            ],
        )?;
        Ok(())
    }

    pub fn list_opportunities(&self, limit: usize) -> Result<Vec<OpportunityRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            r#"
            SELECT id, strategy, mode, slot_detected, detected_at_ms, start_mint,
                   amount_in, amount_out, est_gross_lamports, est_net_lamports,
                   confidence, ttl_slots, dedupe_key, outcome, legs_json, borrow_json,
                   costs_json, meta_json
            FROM opportunities
            ORDER BY detected_at_ms DESC
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit as i64], row_to_opportunity)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn set_opportunity_outcome(&self, id: Uuid, outcome: OutcomeLabel) -> Result<()> {
        self.conn()?.execute(
            "UPDATE opportunities SET outcome = ?1 WHERE id = ?2",
            params![outcome.as_str(), id.to_string()],
        )?;
        Ok(())
    }

    pub fn insert_attempt(&self, attempt: &AttemptRecord) -> Result<()> {
        self.conn()?.execute(
            r#"
            INSERT OR REPLACE INTO attempts (
                id, opportunity_id, ts_ms, strategy, mode, status, bundle_id,
                simulated, landed, reverted, realized_net_lamports, tip_lamports,
                fee_lamports, signature, error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
            params![
                attempt.id,
                attempt.opportunity_id,
                attempt.ts_ms,
                attempt.strategy,
                attempt.mode,
                attempt.status,
                attempt.bundle_id,
                attempt.simulated as i64,
                attempt.landed as i64,
                attempt.reverted as i64,
                attempt.realized_net_lamports,
                attempt.tip_lamports as i64,
                attempt.fee_lamports as i64,
                attempt.signature,
                attempt.error,
            ],
        )?;
        Ok(())
    }

    pub fn update_attempt(&self, id: &str, update: &AttemptUpdate) -> Result<()> {
        self.conn()?.execute(
            r#"
            UPDATE attempts SET
                status = COALESCE(?2, status),
                bundle_id = COALESCE(?3, bundle_id),
                signature = COALESCE(?4, signature),
                error = COALESCE(?5, error),
                realized_net_lamports = COALESCE(?6, realized_net_lamports),
                landed = COALESCE(?7, landed),
                reverted = COALESCE(?8, reverted),
                tip_lamports = COALESCE(?9, tip_lamports),
                fee_lamports = COALESCE(?10, fee_lamports),
                simulated = COALESCE(?11, simulated)
            WHERE id = ?1
            "#,
            params![
                id,
                update.status,
                update.bundle_id,
                update.signature,
                update.error,
                update.realized_net_lamports,
                update.landed.map(|value| value as i64),
                update.reverted.map(|value| value as i64),
                update.tip_lamports.map(|value| value as i64),
                update.fee_lamports.map(|value| value as i64),
                update.simulated.map(|value| value as i64),
            ],
        )?;
        Ok(())
    }

    pub fn get_attempt(&self, id: &str) -> Result<Option<AttemptRecord>> {
        self.conn()?
            .query_row(
                r#"
                SELECT id, opportunity_id, ts_ms, strategy, mode, status, bundle_id,
                       simulated, landed, reverted, realized_net_lamports, tip_lamports,
                       fee_lamports, signature, error
                FROM attempts WHERE id = ?1
                "#,
                params![id],
                row_to_attempt,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn list_attempts(&self, limit: usize) -> Result<Vec<AttemptRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            r#"
            SELECT id, opportunity_id, ts_ms, strategy, mode, status, bundle_id,
                   simulated, landed, reverted, realized_net_lamports, tip_lamports,
                   fee_lamports, signature, error
            FROM attempts
            ORDER BY ts_ms DESC
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit as i64], row_to_attempt)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn insert_bundle(&self, bundle: &BundleRecord) -> Result<()> {
        self.conn()?.execute(
            r#"
            INSERT OR REPLACE INTO bundles (
                bundle_id, attempt_id, ts_ms, status, tip_lamports,
                signatures_json, landed_slot, error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                bundle.bundle_id,
                bundle.attempt_id,
                bundle.ts_ms,
                bundle.status,
                bundle.tip_lamports as i64,
                bundle.signatures_json,
                bundle.landed_slot.map(|slot| slot as i64),
                bundle.error,
            ],
        )?;
        Ok(())
    }

    pub fn update_bundle(
        &self,
        bundle_id: &str,
        status: &str,
        landed_slot: Option<u64>,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn()?.execute(
            r#"
            UPDATE bundles SET status = ?2,
                landed_slot = COALESCE(?3, landed_slot),
                error = COALESCE(?4, error)
            WHERE bundle_id = ?1
            "#,
            params![
                bundle_id,
                status,
                landed_slot.map(|slot| slot as i64),
                error,
            ],
        )?;
        Ok(())
    }

    pub fn list_bundles(&self, limit: usize) -> Result<Vec<BundleRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            r#"
            SELECT bundle_id, attempt_id, ts_ms, status, tip_lamports,
                   signatures_json, landed_slot, error
            FROM bundles ORDER BY ts_ms DESC LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit as i64], row_to_bundle)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn pending_bundles(&self, limit: usize) -> Result<Vec<BundleRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            r#"
            SELECT bundle_id, attempt_id, ts_ms, status, tip_lamports,
                   signatures_json, landed_slot, error
            FROM bundles
            WHERE status IN ('submitted', 'pending', 'landed_pending')
            ORDER BY ts_ms ASC LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit as i64], row_to_bundle)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn list_settlements(&self, limit: usize) -> Result<Vec<SettlementResult>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            r#"
            SELECT opportunity_id, signature, slot, realized_gross_lamports,
                   realized_tip_lamports, realized_fee_lamports,
                   realized_net_lamports, modeled_net_lamports, settled_at_ms
            FROM settlements ORDER BY settled_at_ms DESC LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit as i64], row_to_settlement)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn opportunity_modeled_net(&self, id: &str) -> Result<Option<i64>> {
        self.conn()?
            .query_row(
                "SELECT est_net_lamports FROM opportunities WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn expire_open_opportunities(&self, older_than_ms: i64) -> Result<usize> {
        let changed = self.conn()?.execute(
            "UPDATE opportunities SET outcome = 'open_expired'              WHERE outcome = 'open' AND detected_at_ms < ?1",
            params![older_than_ms],
        )?;
        Ok(changed)
    }

    /// Aggregate modeled stats per strategy for the dashboard/report.
    pub fn strategy_summary(&self) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            r#"
            SELECT strategy, COUNT(*) AS total,
                   SUM(CASE WHEN est_net_lamports > 0 THEN 1 ELSE 0 END) AS profitable,
                   COALESCE(SUM(est_net_lamports), 0) AS modeled_net,
                   COALESCE(MAX(est_net_lamports), 0) AS best_net
            FROM opportunities GROUP BY strategy ORDER BY modeled_net DESC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok(serde_json::json!({
                "strategy": row.get::<_, String>(0)?,
                "opportunities": row.get::<_, i64>(1)?,
                "profitable": row.get::<_, i64>(2)?,
                "modeled_net_lamports": row.get::<_, i64>(3)?,
                "best_net_lamports": row.get::<_, i64>(4)?,
            }))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn record_event(
        &self,
        level: &str,
        component: &str,
        message: &str,
        data: serde_json::Value,
    ) -> Result<()> {
        let data_json = serde_json::to_string(&data)?;
        self.conn()?.execute(
            "INSERT INTO events (ts_ms, level, component, message, data_json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                chrono::Utc::now().timestamp_millis(),
                level,
                component,
                message,
                data_json
            ],
        )?;
        Ok(())
    }

    pub fn recent_events(&self, limit: usize) -> Result<Vec<EventRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            r#"
            SELECT id, ts_ms, level, component, message, data_json
            FROM events ORDER BY id DESC LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit as i64], |row| {
            Ok(EventRecord {
                id: row.get(0)?,
                ts_ms: row.get(1)?,
                level: row.get(2)?,
                component: row.get(3)?,
                message: row.get(4)?,
                data_json: row.get(5)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn insert_settlement(&self, settlement: &SettlementResult) -> Result<()> {
        self.conn()?.execute(
            r#"
            INSERT OR REPLACE INTO settlements (
                opportunity_id, signature, slot, realized_gross_lamports,
                realized_tip_lamports, realized_fee_lamports,
                realized_net_lamports, modeled_net_lamports, settled_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                settlement.opportunity_id.to_string(),
                settlement.signature,
                settlement.slot as i64,
                settlement.realized_gross_lamports,
                settlement.realized_tip_lamports as i64,
                settlement.realized_fee_lamports as i64,
                settlement.realized_net_lamports,
                settlement.modeled_net_lamports,
                settlement.settled_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn stats(&self) -> Result<StoreStats> {
        let conn = self.conn()?;
        let realized_net_lamports = conn
            .query_row(
                "SELECT COALESCE(SUM(realized_net_lamports), 0) FROM attempts WHERE landed = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let last_event_ms = conn
            .query_row("SELECT MAX(ts_ms) FROM events", [], |row| row.get(0))
            .optional()?
            .flatten();
        Ok(StoreStats {
            pools: scalar(&conn, "SELECT COUNT(*) FROM pools")?,
            opportunities: scalar(&conn, "SELECT COUNT(*) FROM opportunities")?,
            attempts: scalar(&conn, "SELECT COUNT(*) FROM attempts")?,
            landed: scalar(&conn, "SELECT COUNT(*) FROM attempts WHERE landed = 1")?,
            reverted: scalar(&conn, "SELECT COUNT(*) FROM attempts WHERE reverted = 1")?,
            bundles: scalar(&conn, "SELECT COUNT(*) FROM bundles")?,
            events: scalar(&conn, "SELECT COUNT(*) FROM events")?,
            settlements: scalar(&conn, "SELECT COUNT(*) FROM settlements")?,
            realized_net_lamports,
            last_event_ms,
        })
    }

    pub fn prune_before(&self, cutoff_ms: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM opportunities WHERE detected_at_ms < ?1",
            params![cutoff_ms],
        )?;
        conn.execute("DELETE FROM attempts WHERE ts_ms < ?1", params![cutoff_ms])?;
        conn.execute("DELETE FROM bundles WHERE ts_ms < ?1", params![cutoff_ms])?;
        conn.execute("DELETE FROM events WHERE ts_ms < ?1", params![cutoff_ms])?;
        Ok(())
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        self.conn()?
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn quick_check(&self) -> Result<String> {
        self.conn()?
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(StoreError::from)
    }
}

fn scalar(conn: &Connection, sql: &str) -> Result<i64> {
    Ok(conn.query_row(sql, [], |row| row.get(0))?)
}

fn parse_address(value: String) -> rusqlite::Result<Address> {
    value.parse::<Address>().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn row_to_pool(row: &Row<'_>) -> rusqlite::Result<PoolSnapshot> {
    let model_json: String = row.get(10)?;
    let model = serde_json::from_str(&model_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(PoolSnapshot {
        address: parse_address(row.get(0)?)?,
        venue: row.get(1)?,
        program_id: parse_address(row.get(2)?)?,
        mint_a: parse_address(row.get(3)?)?,
        mint_b: parse_address(row.get(4)?)?,
        vault_a: parse_address(row.get(5)?)?,
        vault_b: parse_address(row.get(6)?)?,
        decimals_a: row.get::<_, i64>(7)? as u8,
        decimals_b: row.get::<_, i64>(8)? as u8,
        fee_bps: row.get::<_, i64>(9)? as u32,
        model,
        slot: row.get::<_, i64>(11)? as u64,
        confidence: row.get::<_, f64>(12)? as f32,
    })
}

fn row_to_opportunity(row: &Row<'_>) -> rusqlite::Result<OpportunityRecord> {
    Ok(OpportunityRecord {
        id: row.get(0)?,
        strategy: row.get(1)?,
        mode: row.get(2)?,
        slot_detected: row.get::<_, i64>(3)? as u64,
        detected_at_ms: row.get(4)?,
        start_mint: row.get(5)?,
        amount_in: row.get::<_, i64>(6)? as u64,
        amount_out: row.get::<_, i64>(7)? as u64,
        est_gross_lamports: row.get(8)?,
        est_net_lamports: row.get(9)?,
        confidence: row.get::<_, f64>(10)? as f32,
        ttl_slots: row.get::<_, i64>(11)? as u8,
        dedupe_key: row.get(12)?,
        outcome: row.get(13)?,
        legs_json: row.get(14)?,
        costs_json: row.get(16)?,
        meta_json: row.get(17)?,
    })
}

fn row_to_settlement(row: &Row<'_>) -> rusqlite::Result<SettlementResult> {
    let opportunity_id: String = row.get(0)?;
    let parsed = Uuid::parse_str(&opportunity_id).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(SettlementResult {
        opportunity_id: parsed,
        signature: row.get(1)?,
        slot: row.get::<_, i64>(2)? as u64,
        realized_gross_lamports: row.get(3)?,
        realized_tip_lamports: row.get::<_, i64>(4)? as u64,
        realized_fee_lamports: row.get::<_, i64>(5)? as u64,
        realized_net_lamports: row.get(6)?,
        modeled_net_lamports: row.get(7)?,
        settled_at_ms: row.get(8)?,
    })
}

fn row_to_attempt(row: &Row<'_>) -> rusqlite::Result<AttemptRecord> {
    Ok(AttemptRecord {
        id: row.get(0)?,
        opportunity_id: row.get(1)?,
        ts_ms: row.get(2)?,
        strategy: row.get(3)?,
        mode: row.get(4)?,
        status: row.get(5)?,
        bundle_id: row.get(6)?,
        simulated: row.get::<_, i64>(7)? != 0,
        landed: row.get::<_, i64>(8)? != 0,
        reverted: row.get::<_, i64>(9)? != 0,
        realized_net_lamports: row.get(10)?,
        tip_lamports: row.get::<_, i64>(11)? as u64,
        fee_lamports: row.get::<_, i64>(12)? as u64,
        signature: row.get(13)?,
        error: row.get(14)?,
    })
}

fn row_to_bundle(row: &Row<'_>) -> rusqlite::Result<BundleRecord> {
    Ok(BundleRecord {
        bundle_id: row.get(0)?,
        attempt_id: row.get(1)?,
        ts_ms: row.get(2)?,
        status: row.get(3)?,
        tip_lamports: row.get::<_, i64>(4)? as u64,
        signatures_json: row.get(5)?,
        landed_slot: row.get::<_, Option<i64>>(6)?.map(|slot| slot as u64),
        error: row.get(7)?,
    })
}

/// Build an Opportunity from a route-like legs list. This is a helper for the
/// radar, keeping persistence concerns out of the strategy crate.
#[allow(clippy::too_many_arguments)]
pub fn opportunity_from_legs(
    strategy: StrategyKind,
    mode: Mode,
    slot: u64,
    legs: Vec<LegPlan>,
    gross_lamports: i64,
    costs: CostBreakdown,
    confidence: f32,
    dedupe_key: String,
    meta: serde_json::Value,
) -> Opportunity {
    let net = gross_lamports.saturating_sub(costs.total() as i64);
    Opportunity {
        id: Uuid::new_v4(),
        strategy,
        mode,
        slot_detected: slot,
        detected_at_ms: chrono::Utc::now().timestamp_millis(),
        legs,
        est_gross_lamports: gross_lamports,
        est_costs: costs,
        est_net_lamports: net,
        confidence,
        ttl_slots: 2,
        dedupe_key,
        outcome: OutcomeLabel::Open,
        meta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beast_dex::model::PoolModel;

    fn sample_pool() -> PoolSnapshot {
        PoolSnapshot {
            address: "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C"
                .parse::<Address>()
                .unwrap(),
            venue: "raydium_cpmm".into(),
            program_id: "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C"
                .parse::<Address>()
                .unwrap(),
            mint_a: "So11111111111111111111111111111111111111112"
                .parse::<Address>()
                .unwrap(),
            mint_b: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
                .parse::<Address>()
                .unwrap(),
            vault_a: Address::ZERO,
            vault_b: Address::ZERO,
            decimals_a: 9,
            decimals_b: 6,
            fee_bps: 25,
            model: PoolModel::ConstantProduct {
                reserve_a: 1_000_000_000,
                reserve_b: 100_000_000_000,
            },
            slot: 1,
            confidence: 0.99,
        }
    }

    fn sample_opportunity() -> Opportunity {
        opportunity_from_legs(
            StrategyKind::CrossDexArb,
            Mode::Radar,
            123,
            vec![
                LegPlan {
                    venue: "a".into(),
                    pool: "p1".into(),
                    mint_in: "So11111111111111111111111111111111111111112".into(),
                    mint_out: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".into(),
                    amount_in: 1_000_000,
                    min_out: 1_500_000,
                    price_impact_bps: 10,
                },
                LegPlan {
                    venue: "b".into(),
                    pool: "p2".into(),
                    mint_in: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".into(),
                    mint_out: "So11111111111111111111111111111111111111112".into(),
                    amount_in: 1_500_000,
                    min_out: 1_020_000,
                    price_impact_bps: 12,
                },
            ],
            20_000,
            CostBreakdown {
                tip_lamports: 10_000,
                base_fee_lamports: 10_000,
                ..CostBreakdown::default()
            },
            0.9,
            "test:key".into(),
            serde_json::json!({"route": ["p1", "p2"]}),
        )
    }

    #[test]
    fn pool_roundtrip() {
        let store = Store::in_memory().unwrap();
        let pool = sample_pool();
        store.upsert_pool(&pool).unwrap();
        let loaded = store.load_pools(10).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].address, pool.address);
        assert_eq!(loaded[0].model, pool.model);
        assert_eq!(store.get_pool(pool.address).unwrap().unwrap().fee_bps, 25);
    }

    #[test]
    fn opportunity_roundtrip() {
        let store = Store::in_memory().unwrap();
        let opportunity = sample_opportunity();
        store.insert_opportunity(&opportunity).unwrap();
        let rows = store.list_opportunities(10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].strategy, "cross_dex_arb");
        assert_eq!(rows[0].mode, "radar");
        assert_eq!(rows[0].amount_out, 1_020_000);
        assert!(rows[0].legs_json.contains("p1"));
    }

    #[test]
    fn attempt_and_settlement_roundtrip() {
        let store = Store::in_memory().unwrap();
        let attempt = AttemptRecord {
            id: "attempt-1".into(),
            opportunity_id: Some("opp-1".into()),
            ts_ms: 1,
            strategy: "cross_dex_arb".into(),
            mode: "canary".into(),
            status: "submitted".into(),
            bundle_id: Some("bundle-1".into()),
            simulated: true,
            landed: false,
            reverted: false,
            realized_net_lamports: None,
            tip_lamports: 10_000,
            fee_lamports: 10_000,
            signature: None,
            error: None,
        };
        store.insert_attempt(&attempt).unwrap();
        store
            .update_attempt(
                "attempt-1",
                &AttemptUpdate {
                    status: Some("landed".into()),
                    landed: Some(true),
                    realized_net_lamports: Some(5_000),
                    signature: Some("sig".into()),
                    ..AttemptUpdate::default()
                },
            )
            .unwrap();
        let attempts = store.list_attempts(10).unwrap();
        assert_eq!(attempts.len(), 1);
        assert!(attempts[0].landed);
        assert_eq!(attempts[0].realized_net_lamports, Some(5_000));
        let stats = store.stats().unwrap();
        assert_eq!(stats.landed, 1);
        assert_eq!(stats.realized_net_lamports, 5_000);
    }

    #[test]
    fn events_are_ordered() {
        let store = Store::in_memory().unwrap();
        store
            .record_event("info", "test", "one", serde_json::json!({}))
            .unwrap();
        store
            .record_event("warn", "test", "two", serde_json::json!({}))
            .unwrap();
        let events = store.recent_events(10).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message, "two");
    }

    #[test]
    fn quick_check_passes() {
        let store = Store::in_memory().unwrap();
        assert_eq!(store.quick_check().unwrap(), "ok");
    }
}
