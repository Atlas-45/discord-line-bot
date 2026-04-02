# Operations Guide

このドキュメントは `discord-line` の運用手順をまとめたものです。  
セットアップや開発用の説明は [README.md](/Users/subaru/Desktop/文化祭「polaris」/Discord-Line/README.md) を参照してください。

## 1. 運用ルール

- LINE で `質問` を含むメッセージが来たとき、その送信元の Discord スレッドを開始します
- 一度スレッドが開始した送信元は、その後の会話も同じスレッドへ転送されます
- LINE から新着が来ると、そのスレッド名は `未対応` に戻ります
- Discord から LINE に返信しても、自動では `対応済み` にしません
- `/resolve` はスレッドを `対応済み` にしますが、会話の紐付けは残します
- `/close` はスレッドを `対応済み` にし、会話の紐付けを削除して終了します

## 2. 日常起動

Docker Compose を使う前提です。

起動:

```bash
docker compose up -d --build
```

停止:

```bash
docker compose down
```

状態確認:

```bash
docker compose ps
```

ログ確認:

```bash
docker compose logs -f app
docker compose logs -f caddy
```

## 3. 初回確認

起動後に最低限見るポイントです。

- `docker compose ps` で `app` と `caddy` が `Up` になっている
- `docker compose logs -f app` に致命的なエラーが出ていない
- LINE Developers の Webhook URL が正しい
- Discord bot が対象サーバーに参加している

## 4. LINE / Discord 側の運用

### LINE

- Webhook URL は `https://<host>/line/webhook`
- `Use webhook` は `ON`
- `Webhookの再送` は `ON` 推奨

### Discord

- `DISCORD_CHANNEL_ID` はスレッドを作る親チャンネル
- `DISCORD_NOTIFY_CHANNEL_ID` を設定している場合、通知はそのチャンネルに送られます
- bot には `Send Messages`, `Read Message History`, `Create Public Threads` が必要です

## 5. 問い合わせ対応フロー

1. LINE から `質問` を含む最初のメッセージが届く
2. Discord で新しいスレッドが作られる
3. 以後の会話は同じスレッドに継続転送される
4. Discord 側で bot をメンションして返信文を書く
5. 送信確認ボタンで LINE に送る
6. 対応完了なら `/resolve` または `/close`

使い分け:

- `/resolve`: いったん対応済みにするが、同じ相手との会話は同じスレッドで継続
- `/close`: 完全終了。次は再度 `質問` を含むメッセージが来るまで転送しない

## 6. ngrok での検証

これはテスト用です。

```bash
ngrok http 8080
```

- Webhook URL は `https://<ngrok-url>/line/webhook`
- `docker-compose.yml` には `ngrok` 用の `8080:8080` 公開が入っています
- 本番で不要ならこのポート公開は外してください

## 7. よくあるトラブル

### Discord に転送されない

確認順:

1. LINE で実際にメッセージを送っているか
2. 最初のメッセージに `質問` が入っているか
3. `docker compose logs -f app` にエラーが出ていないか
4. `DISCORD_CHANNEL_ID` と `DISCORD_NOTIFY_CHANNEL_ID` が正しいか
5. `DISCORD_WEBHOOK_URL` が古くないか

### `Unknown Channel (10003)` が出る

- Discord 側でスレッドを手動削除した可能性があります
- 現在の実装では、削除済みスレッドを検知するとマッピングを捨てて新しく作り直します
- それでも出る場合は `DISCORD_WEBHOOK_URL` の古さを疑ってください

### SQLite エラーで起動しない

- Docker では `/data/data.sqlite` を使います
- `docker compose up -d --build` で作り直してください

## 8. 本番反映

コード変更反映:

```bash
docker compose down
docker compose up -d --build
```

テスト:

```bash
cargo test
```
