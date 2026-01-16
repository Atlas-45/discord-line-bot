# discord-line

LINEのWebhookを受け取り、Discordのスレッドへ転送し、DiscordからLINEへ返信するためのRustサービスです。

## 必要なもの

- Rust（stable）

## 環境変数

`.env` を作成するか、起動時に環境変数を渡してください（`dotenvy`で読み込みます）。

**必須**

- `LINE_CHANNEL_SECRET`
- `LINE_CHANNEL_ACCESS_TOKEN`
- `DISCORD_BOT_TOKEN`
- `DISCORD_GUILD_ID`
- `DISCORD_CHANNEL_ID`

**任意**

- `DISCORD_WEBHOOK_URL`（Webhook経由で投稿する場合）
- `DISCORD_NOTIFY_CHANNEL_ID`（通知用チャンネル）
- `DATABASE_URL`（デフォルト: `sqlite://data.sqlite`）
- `BIND_ADDR`（デフォルト: `0.0.0.0:8080`）

## 起動方法（ローカル）

```bash
RUST_LOG=info \
LINE_CHANNEL_SECRET=... \
LINE_CHANNEL_ACCESS_TOKEN=... \
DISCORD_BOT_TOKEN=... \
DISCORD_GUILD_ID=... \
DISCORD_CHANNEL_ID=... \
cargo run
```

Webhookの受け口は `http://<BIND_ADDR>/line/webhook` です。

## ビルド

```bash
cargo build
cargo build --release
```

## テスト

```bash
cargo test
```
