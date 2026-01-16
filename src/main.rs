use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context as AnyhowContext, Result};
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::{DateTime, FixedOffset, Utc};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serenity::async_trait;
use serenity::model::channel::ChannelType;
use serenity::model::prelude::{Channel, Message};
use serenity::prelude::*;
use sha2::Sha256;
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use tracing::{error, info, warn};

const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_DATABASE_URL: &str = "sqlite://data.sqlite";

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;
    let http = Client::new();
    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .context("connect to sqlite")?;

    init_db(&db).await?;

    let state = Arc::new(AppState { config, http, db });

    let app = Router::new()
        .route("/line/webhook", post(line_webhook))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(state.config.bind_addr).await?;
    let server_handle = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            error!(?err, "http server error");
        }
    });

    let intents =
        GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT | GatewayIntents::GUILDS;

    let handler = DiscordHandler {
        state: state.clone(),
    };

    let mut client = serenity::Client::builder(&state.config.discord_bot_token, intents)
        .event_handler(handler)
        .await
        .context("build discord client")?;

    if let Err(err) = client.start().await {
        error!(?err, "discord client ended");
    }

    server_handle.abort();
    Ok(())
}

#[derive(Clone)]
struct Config {
    line_channel_secret: String,
    line_channel_access_token: String,
    discord_bot_token: String,
    discord_guild_id: u64,
    discord_channel_id: u64,
    discord_webhook_url: Option<String>,
    discord_notify_channel_id: Option<u64>,
    database_url: String,
    bind_addr: SocketAddr,
}

impl Config {
    fn from_env() -> Result<Self> {
        let line_channel_secret = env_var("LINE_CHANNEL_SECRET")?;
        let line_channel_access_token = env_var("LINE_CHANNEL_ACCESS_TOKEN")?;
        let discord_bot_token = env_var("DISCORD_BOT_TOKEN")?;
        let discord_guild_id = env_var("DISCORD_GUILD_ID")?.parse::<u64>()?;
        let discord_channel_id = env_var("DISCORD_CHANNEL_ID")?.parse::<u64>()?;
        let discord_webhook_url = std::env::var("DISCORD_WEBHOOK_URL").ok();
        let discord_notify_channel_id = match std::env::var("DISCORD_NOTIFY_CHANNEL_ID") {
            Ok(value) => Some(
                value
                    .parse::<u64>()
                    .context("parse DISCORD_NOTIFY_CHANNEL_ID")?,
            ),
            Err(_) => None,
        };
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string());
        let bind_addr = std::env::var("BIND_ADDR")
            .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string())
            .parse::<SocketAddr>()
            .context("parse BIND_ADDR")?;

        Ok(Self {
            line_channel_secret,
            line_channel_access_token,
            discord_bot_token,
            discord_guild_id,
            discord_channel_id,
            discord_webhook_url,
            discord_notify_channel_id,
            database_url,
            bind_addr,
        })
    }
}

fn env_var(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing env var {key}"))
}

struct AppState {
    config: Config,
    http: Client,
    db: SqlitePool,
}

async fn init_db(db: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS line_threads (
            source_type TEXT NOT NULL,
            source_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (source_type, source_id)
        )",
    )
    .execute(db)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS reply_tokens (
            thread_id TEXT NOT NULL,
            reply_token TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            used INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(db)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS processed_events (
            event_id TEXT PRIMARY KEY,
            received_at INTEGER NOT NULL
        )",
    )
    .execute(db)
    .await?;

    Ok(())
}

async fn line_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let signature = match headers.get("x-line-signature") {
        Some(value) => match value.to_str() {
            Ok(value) => value.to_string(),
            Err(_) => return StatusCode::BAD_REQUEST,
        },
        None => return StatusCode::BAD_REQUEST,
    };

    if !verify_line_signature(&state.config.line_channel_secret, &body, &signature) {
        warn!("line signature mismatch");
        return StatusCode::UNAUTHORIZED;
    }

    let payload: LineWebhookRequest = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(err) => {
            error!(?err, "failed to parse line webhook");
            return StatusCode::BAD_REQUEST;
        }
    };

    for event in payload.events {
        if let Err(err) = process_line_event(state.clone(), event).await {
            error!(?err, "failed to handle line event");
        }
    }

    StatusCode::OK
}

fn verify_line_signature(secret: &str, body: &[u8], signature: &str) -> bool {
    let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
        Ok(mac) => mac,
        Err(_) => return false,
    };
    mac.update(body);
    let decoded = match STANDARD.decode(signature) {
        Ok(decoded) => decoded,
        Err(_) => return false,
    };
    mac.verify_slice(&decoded).is_ok()
}

async fn process_line_event(state: Arc<AppState>, event: LineEvent) -> Result<()> {
    if event.event_type != "message" {
        return Ok(());
    }

    let message = match event.message {
        Some(message) => message,
        None => return Ok(()),
    };

    if message.message_type != "text" {
        return Ok(());
    }

    let text = match message.text {
        Some(text) => text,
        None => return Ok(()),
    };

    if let Some(event_id) = event.webhook_event_id.as_deref() {
        let inserted = mark_event_processed(&state.db, event_id).await?;
        if !inserted {
            info!(event_id, "line event already processed");
            return Ok(());
        }
    }

    let (source_type, source_id) = match event.source.to_key() {
        Some(key) => key,
        None => return Ok(()),
    };

    let thread_id = ensure_discord_thread(&state, &source_type, &source_id).await?;

    if let Some(reply_token) = event.reply_token {
        store_reply_token(&state.db, thread_id, &reply_token).await?;
    }

    let timestamp = format_line_timestamp(event.timestamp);
    let thread_content = format!("{}\nTime: {}", text, timestamp);
    let message_id = send_discord_message(&state, thread_id, &thread_content).await?;

    if let Some(notify_channel_id) = state.config.discord_notify_channel_id {
        let guild_id = state.config.discord_guild_id;
        let message_link = format!(
            "https://discord.com/channels/{}/{}/{}",
            guild_id, thread_id, message_id
        );
        let summary = format!(
            "{}
{}
> {}

{} {}
{} [{}]({})
{}",
            "\u{1F4E9} **LINE \u{30E1}\u{30C3}\u{30BB}\u{30FC}\u{30B8}\u{53D7}\u{4FE1}**",
            "\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}",
            text,
            "\u{1F550}",
            timestamp,
            "\u{1F517}",
            "\u{30E1}\u{30C3}\u{30BB}\u{30FC}\u{30B8}\u{3092}\u{898B}\u{308B}",
            message_link,
            "\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}"
        );
        send_discord_channel_message(&state, notify_channel_id, &summary).await?;
    }

    Ok(())
}

async fn ensure_discord_thread(
    state: &AppState,
    source_type: &str,
    source_id: &str,
) -> Result<u64> {
    if let Some(thread_id) = get_thread_id(&state.db, source_type, source_id).await? {
        return Ok(thread_id);
    }

    let thread_name = format!("LINE-{}", short_id(source_id));
    let thread_id = discord_create_thread(state, &thread_name).await?;

    upsert_thread(&state.db, source_type, source_id, thread_id).await?;

    let intro = format!("LINE source: {source_type}/{source_id}");
    if let Err(err) = send_discord_message(state, thread_id, &intro).await {
        warn!(?err, "failed to send intro message to discord");
    }

    Ok(thread_id)
}

fn short_id(source_id: &str) -> String {
    if source_id.len() <= 6 {
        source_id.to_string()
    } else {
        source_id[source_id.len() - 6..].to_string()
    }
}

async fn send_discord_message(state: &AppState, thread_id: u64, content: &str) -> Result<u64> {
    if let Some(webhook_url) = &state.config.discord_webhook_url {
        let payload = json!({ "content": content });
        let url = format!("{webhook_url}?thread_id={thread_id}&wait=true");
        let response = state.http.post(url).json(&payload).send().await?;
        if response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            let msg: DiscordMessageResponse = serde_json::from_str(&body)?;
            return Ok(parse_discord_id(&msg.id)?);
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("discord webhook error {status}: {body}"));
    }

    send_discord_channel_message_with_id(state, thread_id, content).await
}

async fn send_discord_channel_message(
    state: &AppState,
    channel_id: u64,
    content: &str,
) -> Result<()> {
    send_discord_channel_message_with_id(state, channel_id, content).await?;
    Ok(())
}

async fn send_discord_channel_message_with_id(
    state: &AppState,
    channel_id: u64,
    content: &str,
) -> Result<u64> {
    let payload = json!({ "content": content });
    let url = format!("https://discord.com/api/v10/channels/{channel_id}/messages");
    let response = state
        .http
        .post(url)
        .header(
            "Authorization",
            format!("Bot {}", state.config.discord_bot_token),
        )
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        let msg: DiscordMessageResponse = serde_json::from_str(&body)?;
        return Ok(parse_discord_id(&msg.id)?);
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(anyhow!("discord message error {status}: {body}"))
}

async fn discord_create_thread(state: &AppState, name: &str) -> Result<u64> {
    let url = format!(
        "https://discord.com/api/v10/channels/{}/threads",
        state.config.discord_channel_id
    );
    let payload = json!({
        "name": name,
        "auto_archive_duration": 60,
        "type": 11
    });

    let response = state
        .http
        .post(url)
        .header(
            "Authorization",
            format!("Bot {}", state.config.discord_bot_token),
        )
        .json(&payload)
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("discord thread error {status}: {body}"));
    }

    let channel: DiscordChannelResponse = serde_json::from_str(&body)?;
    Ok(parse_discord_id(&channel.id)?)
}

fn parse_discord_id(id: &str) -> Result<u64> {
    id.parse::<u64>().context("parse discord id")
}

async fn get_thread_id(db: &SqlitePool, source_type: &str, source_id: &str) -> Result<Option<u64>> {
    let record =
        sqlx::query("SELECT thread_id FROM line_threads WHERE source_type = ? AND source_id = ?")
            .bind(source_type)
            .bind(source_id)
            .fetch_optional(db)
            .await?;

    let thread_id = match record {
        Some(row) => {
            let value: String = row.try_get("thread_id")?;
            Some(parse_discord_id(&value)?)
        }
        None => None,
    };

    Ok(thread_id)
}

async fn upsert_thread(
    db: &SqlitePool,
    source_type: &str,
    source_id: &str,
    thread_id: u64,
) -> Result<()> {
    let now = now_ts();
    sqlx::query(
        "INSERT INTO line_threads (source_type, source_id, thread_id, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(source_type, source_id) DO UPDATE SET
            thread_id = excluded.thread_id,
            updated_at = excluded.updated_at",
    )
    .bind(source_type)
    .bind(source_id)
    .bind(thread_id.to_string())
    .bind(now)
    .execute(db)
    .await?;

    Ok(())
}

async fn store_reply_token(db: &SqlitePool, thread_id: u64, reply_token: &str) -> Result<()> {
    let now = now_ts();
    sqlx::query(
        "INSERT INTO reply_tokens (thread_id, reply_token, created_at, used)
         VALUES (?, ?, ?, 0)",
    )
    .bind(thread_id.to_string())
    .bind(reply_token)
    .bind(now)
    .execute(db)
    .await?;

    Ok(())
}

async fn mark_event_processed(db: &SqlitePool, event_id: &str) -> Result<bool> {
    let now = now_ts();
    let result = sqlx::query(
        "INSERT OR IGNORE INTO processed_events (event_id, received_at)
         VALUES (?, ?)",
    )
    .bind(event_id)
    .bind(now)
    .execute(db)
    .await?;

    Ok(result.rows_affected() == 1)
}

async fn latest_reply_token(db: &SqlitePool, thread_id: u64) -> Result<Option<String>> {
    let record = sqlx::query(
        "SELECT reply_token FROM reply_tokens
         WHERE thread_id = ? AND used = 0
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(thread_id.to_string())
    .fetch_optional(db)
    .await?;

    let reply_token = match record {
        Some(row) => Some(row.try_get("reply_token")?),
        None => None,
    };
    Ok(reply_token)
}

async fn mark_reply_token_used(db: &SqlitePool, reply_token: &str) -> Result<()> {
    sqlx::query("UPDATE reply_tokens SET used = 1 WHERE reply_token = ?")
        .bind(reply_token)
        .execute(db)
        .await?;
    Ok(())
}

async fn get_line_source_by_thread(
    db: &SqlitePool,
    thread_id: u64,
) -> Result<Option<(String, String)>> {
    let record = sqlx::query("SELECT source_type, source_id FROM line_threads WHERE thread_id = ?")
        .bind(thread_id.to_string())
        .fetch_optional(db)
        .await?;

    let mapping = match record {
        Some(row) => Some((row.try_get("source_type")?, row.try_get("source_id")?)),
        None => None,
    };

    Ok(mapping)
}

async fn send_line_reply(
    state: &AppState,
    reply_token: &str,
    content: &str,
) -> Result<LineReplyOutcome> {
    let url = "https://api.line.me/v2/bot/message/reply";
    let payload = LineReplyRequest {
        reply_token,
        messages: vec![LineTextMessage::new(content)],
    };

    let response = state
        .http
        .post(url)
        .header(
            "Authorization",
            format!("Bearer {}", state.config.line_channel_access_token),
        )
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        return Ok(LineReplyOutcome::Sent);
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status == StatusCode::BAD_REQUEST {
        if body.contains("Invalid reply token") {
            return Ok(LineReplyOutcome::InvalidToken);
        }
    }

    Err(anyhow!("line reply error {status}: {body}"))
}

async fn send_line_push(state: &AppState, target: &str, content: &str) -> Result<()> {
    let url = "https://api.line.me/v2/bot/message/push";
    let payload = LinePushRequest {
        to: target,
        messages: vec![LineTextMessage::new(content)],
    };

    let response = state
        .http
        .post(url)
        .header(
            "Authorization",
            format!("Bearer {}", state.config.line_channel_access_token),
        )
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(anyhow!("line push error {status}: {body}"))
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn format_line_timestamp(timestamp_ms: Option<i64>) -> String {
    let Some(timestamp_ms) = timestamp_ms else {
        return "unknown".to_string();
    };

    let Some(utc) = DateTime::<Utc>::from_timestamp_millis(timestamp_ms) else {
        return timestamp_ms.to_string();
    };

    let jst = utc.with_timezone(&FixedOffset::east_opt(9 * 3600).unwrap());
    jst.format("%Y-%m-%d %H:%M:%S %:z").to_string()
}

#[derive(Deserialize)]
struct LineWebhookRequest {
    events: Vec<LineEvent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LineEvent {
    #[serde(rename = "type")]
    event_type: String,
    reply_token: Option<String>,
    source: LineSource,
    message: Option<LineMessage>,
    webhook_event_id: Option<String>,
    timestamp: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LineSource {
    #[serde(rename = "type")]
    source_type: String,
    user_id: Option<String>,
    group_id: Option<String>,
    room_id: Option<String>,
}

impl LineSource {
    fn to_key(&self) -> Option<(String, String)> {
        let source_id = self
            .user_id
            .clone()
            .or_else(|| self.group_id.clone())
            .or_else(|| self.room_id.clone())?;
        Some((self.source_type.clone(), source_id))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LineMessage {
    #[serde(rename = "type")]
    message_type: String,
    text: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LineReplyRequest<'a> {
    reply_token: &'a str,
    messages: Vec<LineTextMessage<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LinePushRequest<'a> {
    to: &'a str,
    messages: Vec<LineTextMessage<'a>>,
}

#[derive(Serialize)]
struct LineTextMessage<'a> {
    #[serde(rename = "type")]
    message_type: &'static str,
    text: &'a str,
}

impl<'a> LineTextMessage<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            message_type: "text",
            text,
        }
    }
}

#[derive(Deserialize)]
struct DiscordChannelResponse {
    id: String,
}

#[derive(Deserialize)]
struct DiscordMessageResponse {
    id: String,
}

enum LineReplyOutcome {
    Sent,
    InvalidToken,
}

struct DiscordHandler {
    state: Arc<AppState>,
}

#[async_trait]
impl EventHandler for DiscordHandler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot || msg.webhook_id.is_some() {
            return;
        }

        if msg.content.trim().is_empty() {
            return;
        }

        let channel = match msg.channel_id.to_channel(&ctx.http).await {
            Ok(channel) => channel,
            Err(err) => {
                warn!(?err, "failed to fetch channel");
                return;
            }
        };

        let thread_id = match channel {
            Channel::Guild(channel) => {
                let is_thread = matches!(
                    channel.kind,
                    ChannelType::PublicThread | ChannelType::PrivateThread
                );
                if !is_thread {
                    return;
                }
                let parent_id = channel.parent_id.map(|id| id.get());
                if parent_id != Some(self.state.config.discord_channel_id) {
                    return;
                }
                channel.id.get()
            }
            _ => return,
        };

        let source = match get_line_source_by_thread(&self.state.db, thread_id).await {
            Ok(Some(source)) => source,
            Ok(None) => {
                warn!(thread_id, "no line mapping for thread");
                return;
            }
            Err(err) => {
                error!(?err, "failed to load line mapping");
                return;
            }
        };

        if let Err(err) =
            send_line_from_discord(&self.state, thread_id, &source, &msg.content).await
        {
            error!(?err, "failed to send line reply");
        }
    }
}

async fn send_line_from_discord(
    state: &AppState,
    thread_id: u64,
    source: &(String, String),
    content: &str,
) -> Result<()> {
    let target = &source.1;

    let mut push_method = "push";

    if let Some(reply_token) = latest_reply_token(&state.db, thread_id).await? {
        let outcome = send_line_reply(state, &reply_token, content).await?;
        mark_reply_token_used(&state.db, &reply_token).await?;

        match outcome {
            LineReplyOutcome::Sent => {
                send_api_notice(state, thread_id, "reply").await?;
                return Ok(());
            }
            LineReplyOutcome::InvalidToken => {
                info!("reply token invalid, falling back to push");
                push_method = "push (fallback)";
            }
        }
    }

    send_line_push(state, target, content).await?;
    send_api_notice(state, thread_id, push_method).await?;
    Ok(())
}

async fn send_api_notice(state: &AppState, thread_id: u64, method: &str) -> Result<()> {
    let notice = format!("API: {method}");
    send_discord_channel_message(state, thread_id, &notice).await
}
