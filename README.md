# discord-line

LINEのWebhookを受け取り、Discordのスレッドへ転送し、DiscordからLINEへ返信するためのRustサービスです。

## 主な機能

- LINEメッセージをDiscordのスレッドへ転送（スレッド自動作成・再利用）
- Discordスレッドの返信をLINEへ送信（replyを優先し、失敗時はpushにフォールバック）
- 任意で通知チャンネルへ新着を送信（時刻付き）
- webhookEventIdで重複イベントを抑止

## 必要なもの

- Rust（stable）
- LINE Messaging APIチャネル
- Discord Bot（Message Content Intentを有効化）

## 環境変数

`.env` を作成するか、起動時に環境変数を渡してください（`dotenvy`で読み込みます）。

**必須**

- `LINE_CHANNEL_SECRET`
- `LINE_CHANNEL_ACCESS_TOKEN`
- `DISCORD_BOT_TOKEN`
- `DISCORD_CHANNEL_ID`（スレッドを作成する親チャンネルID）

**任意**

- `DISCORD_WEBHOOK_URL`（Webhook経由で投稿する場合）
- `DISCORD_NOTIFY_CHANNEL_ID`（通知用チャンネルID）
- `DATABASE_URL`（デフォルト: `sqlite://data.sqlite`）
- `BIND_ADDR`（デフォルト: `0.0.0.0:8080`）
- `RUST_LOG`（例: `info`）

`.env` の例:

```dotenv
LINE_CHANNEL_SECRET=...
LINE_CHANNEL_ACCESS_TOKEN=...
DISCORD_BOT_TOKEN=...
DISCORD_CHANNEL_ID=123456789012345678
# 通知チャンネルを使う場合
# DISCORD_NOTIFY_CHANNEL_ID=123456789012345678
# Webhook送信を使う場合
# DISCORD_WEBHOOK_URL=https://discord.com/api/webhooks/...
# 永続ファイルに保存する場合
# DATABASE_URL=sqlite:///var/lib/discord-line/data.sqlite
BIND_ADDR=127.0.0.1:8080
RUST_LOG=info
```

## 起動方法（ローカル）

```bash
cargo run
```

Webhookの受け口は `http://<BIND_ADDR>/line/webhook` です（本番はHTTPS必須）。

## LINEのWebhook設定

- Webhook URL: `https://<your-domain>/line/webhook`
- 「Use webhook」をON
- 「Webhookの再送」をON推奨（重複イベントはDBで抑止）

## Discord側の設定

- Botの権限: `Send Messages`, `Read Message History`, `Create Public Threads`
- Privileged Gateway Intents: **Message Content Intent をON**

## ビルド

```bash
cargo build
cargo build --release
```

## サーバー反映

```bash
cd /opt/discord-line
git pull origin main
cargo build --release
sudo systemctl stop discord-line
sudo cp target/release/discord-line /opt/discord-line/discord-line
sudo systemctl start discord-line
sudo journalctl -u discord-line -n 20 --no-pager
```

## テスト

```bash
cargo test
```
