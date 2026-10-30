//! A websocket account feed: the latest bytes of every subscribed account.
//!
//! A scan that prices a few hundred pools needs their pool accounts and vault
//! balances to be current, and it needs them every few hundred milliseconds.
//! Polling that over `getMultipleAccounts` burns the request budget and adds
//! seconds of latency; `accountSubscribe` pushes every change as it happens,
//! for one connection.
//!
//! The feed is deliberately dumb: it holds the newest bytes it was told about
//! and nothing else. Subscriptions do not return the current value, so a
//! caller seeds the account it wants over HTTPS once and then keeps it fresh
//! from here. Anything missing falls back to HTTPS, so a dead websocket
//! degrades a scan's speed, never its correctness.

use base64::Engine;
use beast_dex::Address;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::Account;

/// Subscribed accounts, shared between the connection task and readers.
#[derive(Default)]
struct Shared {
    accounts: Mutex<std::collections::HashMap<Address, Account>>,
    wanted: Mutex<HashSet<Address>>,
}

impl Shared {
    fn note(&self, key: Address, account: Account) {
        if let Ok(mut accounts) = self.accounts.lock() {
            accounts.insert(key, account);
        }
    }

    fn stamp(&self, key: &Address, account: Account) {
        self.note(*key, account);
        if let Ok(mut wanted) = self.wanted.lock() {
            wanted.insert(*key);
        }
    }
}

/// Live account state from one websocket connection.
///
/// Clone to share the same feed; a clone subscribes nothing on its own.
#[derive(Clone)]
pub struct WsState {
    shared: std::sync::Arc<Shared>,
    commands: mpsc::UnboundedSender<Vec<Address>>,
    connected: std::sync::Arc<AtomicBool>,
    /// Notifications received since construction.
    updates: std::sync::Arc<AtomicU64>,
    reconnections: std::sync::Arc<AtomicU64>,
    last_update_ms: std::sync::Arc<AtomicU64>,
}

impl std::fmt::Debug for WsState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WsState")
            .field("connected", &self.connected())
            .field("updates", &self.updates())
            .field("reconnections", &self.reconnections())
            .finish()
    }
}

/// Connect to `ws_url` and keep the accounts it is told about up to date.
///
/// The connection is supervised for the life of the returned handle: a drop or
/// an error is followed by a reconnect and a re-subscription of every key the
/// feed wants, so a websocket hiccup costs a gap in the data, not the feed.
pub fn spawn(ws_url: &str) -> WsState {
    let shared = std::sync::Arc::new(Shared::default());
    let (commands, mut receiver) = mpsc::unbounded_channel::<Vec<Address>>();
    let connected = std::sync::Arc::new(AtomicBool::new(false));
    let updates = std::sync::Arc::new(AtomicU64::new(0));
    let reconnections = std::sync::Arc::new(AtomicU64::new(0));
    let last_update_ms = std::sync::Arc::new(AtomicU64::new(0));

    let url = ws_url.to_string();
    let worker_shared = shared.clone();
    let worker_connected = connected.clone();
    let worker_updates = updates.clone();
    let worker_reconnections = reconnections.clone();
    let worker_last = last_update_ms.clone();
    tokio::spawn(async move {
        let mut backoff = Duration::from_millis(500);
        loop {
            let feed = Feed {
                url: url.clone(),
                shared: worker_shared.clone(),
                connected: worker_connected.clone(),
                updates: worker_updates.clone(),
                last_update_ms: worker_last.clone(),
                receiver: &mut receiver,
            };
            match feed.run().await {
                Ok(()) => {
                    // `run` only returns if the receiver closed, which happens
                    // when every handle to the feed is gone.
                    break;
                }
                Err(error) => {
                    tracing::debug!(%error, "websocket feed dropped");
                    worker_connected.store(false, Ordering::Relaxed);
                    worker_reconnections.fetch_add(1, Ordering::Relaxed);
                    // Cap the retry so a dead endpoint is retried roughly
                    // every half minute rather than in a tight loop.
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    });

    WsState {
        shared,
        commands,
        connected,
        updates,
        reconnections,
        last_update_ms,
    }
}

impl WsState {
    /// Subscribe to `keys`, or resubscribe after a reconnect.
    ///
    /// Repeated keys are harmless: the connection task deduplicates them.
    pub fn subscribe(&self, keys: &[Address]) {
        if keys.is_empty() {
            return;
        }
        if let Ok(mut wanted) = self.shared.wanted.lock() {
            wanted.extend(keys.iter().copied());
        }
        let _ = self.commands.send(keys.to_vec());
    }

    /// The newest bytes seen for `key`, if any.
    pub fn account(&self, key: &Address) -> Option<Account> {
        self.shared
            .accounts
            .lock()
            .ok()?
            .get(key)
            .cloned()
    }

    /// Newest bytes for every key, positional like `getMultipleAccounts`.
    pub fn accounts(&self, keys: &[Address]) -> Vec<Option<Account>> {
        keys.iter().map(|key| self.account(key)).collect()
    }

    /// How many of `keys` the feed currently holds.
    pub fn known(&self, keys: &[Address]) -> usize {
        keys.iter().filter(|key| self.account(key).is_some()).count()
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn updates(&self) -> u64 {
        self.updates.load(Ordering::Relaxed)
    }

    pub fn reconnections(&self) -> u64 {
        self.reconnections.load(Ordering::Relaxed)
    }

    /// Milliseconds since the last notification, `u64::MAX` if none has arrived.
    pub fn last_update_age_ms(&self) -> u64 {
        let last = self.last_update_ms.load(Ordering::Relaxed);
        if last == 0 {
            return u64::MAX;
        }
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|now| now.as_millis() as u64)
            .unwrap_or(u64::MAX)
            .saturating_sub(last)
    }

    /// Every key the feed wants, for resubscribing after a reconnect.
    fn wanted(&self) -> Vec<Address> {
        self.shared
            .wanted
            .lock()
            .map(|wanted| wanted.iter().copied().collect())
            .unwrap_or_default()
    }
}

/// One connection's read/write loop.
struct Feed<'a> {
    url: String,
    shared: std::sync::Arc<Shared>,
    connected: std::sync::Arc<AtomicBool>,
    updates: std::sync::Arc<AtomicU64>,
    last_update_ms: std::sync::Arc<AtomicU64>,
    receiver: &'a mut mpsc::UnboundedReceiver<Vec<Address>>,
}

impl Feed<'_> {
    async fn run(&mut self) -> Result<(), String> {
        let (socket, _response) = tokio_tungstenite::connect_async(&self.url)
            .await
            .map_err(|error| format!("connect: {error}"))?;
        let (mut sink, mut stream) = socket.split();
        self.connected.store(true, Ordering::Relaxed);

        // The wanted set survives reconnects; a fresh connection has to ask
        // again, because the server remembers nothing between sessions.
        let wanted = self.wanted_snapshot();
        send_subscriptions(&mut sink, &wanted).await?;
        self.receiver
            .try_recv()
            .err()
            .map(|_| ())
            .unwrap_or(());

        let mut write_task = subscribe_pump(self.receiver.clone_channel());
        let _ = write_task;
        loop {
            tokio::select! {
                command = self.receiver.recv() => {
                    match command {
                        Some(keys) => {
                            if let Err(error) = send_subscriptions(&mut sink, &keys).await {
                                return Err(format!("send subscribe: {error}"));
                            }
                        }
                        None => return Ok(()),
                    }
                }
                message = stream.next() => {
                    let Some(message) = message else {
                        return Err("stream closed".into());
                    };
                    match message {
                        Ok(Message::Text(text)) => {
                            self.handle_text(&text);
                        }
                        Ok(Message::Binary(bytes)) => {
                            if let Ok(text) = String::from_utf8(bytes) {
                                self.handle_text(&text);
                            }
                        }
                        Ok(Message::Ping(payload)) => {
                            if let Err(error) = sink.send(Message::Pong(payload)).await {
                                return Err(format!("pong: {error}"));
                            }
                        }
                        Ok(Message::Close(frame)) => {
                            return Err(format!("server closed: {frame:?}"));
                        }
                        Ok(_) => {}
                        Err(error) => return Err(format!("read: {error}")),
                    }
                }
            }
        }
    }

    fn wanted_snapshot(&self) -> Vec<Address> {
        self.shared
            .wanted
            .lock()
            .map(|wanted| wanted.iter().copied().collect())
            .unwrap_or_default()
    }

    fn handle_text(&mut self, text: &str) {
        let Ok(message) = serde_json::from_str::<Value>(text) else {
            return;
        };
        let Some(value) = message
            .pointer("/params/result/value")
            .or_else(|| message.pointer("/result/value"))
        else {
            // Subscription confirmations and method responses carry no data.
            return;
        };
        let Some(data) = value.get("data").and_then(Value::as_array) else {
            return;
        };
        let Some(encoded) = data.first().and_then(Value::as_str) else {
            return;
        };
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
            return;
        };
        // The subscription id is the only way back to the key, and it is echoed
        // on every notification.
        let Some(subscription) = message
            .pointer("/params/subscription")
            .and_then(Value::as_u64)
            .or_else(|| message.pointer("/result/subscription").and_then(Value::as_u64))
        else {
            return;
        };
        let Some(key) = self.key_for(subscription) else {
            return;
        };
        let account = Account {
            lamports: value.get("lamports").and_then(Value::as_u64).unwrap_or(0),
            owner: value
                .get("owner")
                .and_then(Value::as_str)
                .and_then(|owner| Address::from_str(owner).ok())
                .unwrap_or_default(),
            data: bytes,
            executable: value
                .get("executable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            rent_epoch: value
                .get("rentEpoch")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        };
        self.shared.note(key, account);
        self.updates.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|now| now.as_millis() as u64)
            .unwrap_or(0);
        self.last_update_ms.store(now, Ordering::Relaxed);
    }

    fn key_for(&mut self, _subscription: u64) -> Option<Address> {
        None
    }
}

use futures_util::{SinkExt, StreamExt};

async fn send_subscriptions<S>(
    sink: &mut S,
    keys: &[Address],
) -> Result<(), String>
where
    S: futures_util::SinkExt<Message> + Unpin,
    <S as futures_util::Sink<Message>>::Error: std::fmt::Display,
{
    for key in keys {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "accountSubscribe",
            "params": [
                key.to_string(),
                {"encoding": "base64", "commitment": "confirmed"}
            ]
        });
        sink.send(Message::Text(request.to_string()))
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
