# AGENTS.md

This file guides agentic coding assistants in this repository.
日本語メモ: このリポジトリで作業するエージェント向けの指針です。

## Scope / スコープ
- Applies to the entire repository.
- Target is a Rust service bridging LINE and Discord.
- Keep changes minimal and focused on the task.
- If you add new files, update this guidance when needed.

## Project Overview / 概要
- Rust service bridging LINE webhooks and Discord threads.
- Uses Axum for HTTP, Serenity for Discord, SQLx + SQLite for storage.
- Runs as a long-lived service; env vars are required.

## Commands / コマンド
### Build
- `cargo build`
- `cargo build --release`

### Run (local)
- `cargo run`
- Use `RUST_LOG=info` (or `debug`) for logs.
- Example: `RUST_LOG=info LINE_CHANNEL_SECRET=... DISCORD_BOT_TOKEN=... cargo run`

### Test
- `cargo test`
- Single test by name: `cargo test <test_name>`
- Single module test: `cargo test module::test_name`
- Doc tests: `cargo test --doc`
- Integration test file: `cargo test --test <file>`

### Single-test examples
- Unit test function: `cargo test process_line_event`
- Specific module path: `cargo test line::tests::signature`
- Show logs: `cargo test <name> -- --nocapture`

### Lint / Format
- `cargo fmt`
- `cargo fmt --check` (CI-style)
- `cargo clippy --all-targets --all-features -- -D warnings`

### Utility
- `cargo clean` when build cache is stale.

## Environment Variables / 環境変数
- Required: `LINE_CHANNEL_SECRET`, `LINE_CHANNEL_ACCESS_TOKEN`.
- Required: `DISCORD_BOT_TOKEN`, `DISCORD_GUILD_ID`, `DISCORD_CHANNEL_ID`.
- Optional: `DISCORD_WEBHOOK_URL` (webhook posting).
- Optional: `DISCORD_NOTIFY_CHANNEL_ID` (summary channel).
- Optional: `DATABASE_URL` (default `sqlite://data.sqlite`).
- Optional: `BIND_ADDR` (default `0.0.0.0:8080`).
- `.env` is loaded via `dotenvy`.

## Code Style / コードスタイル
### Imports
- Order: `std` first, blank line, external crates, blank line, local modules.
- Prefer explicit imports; use glob only for prelude-style crates.
- Keep unused imports removed.

### Formatting
- Rustfmt defaults; 4-space indent.
- Use trailing commas in multi-line structs, enums, builder chains.
- Break long chains with one method per line.
- Keep SQL strings aligned and indented consistently.

### Naming
- `snake_case` for functions/vars, `PascalCase` for types.
- Constants in `SCREAMING_SNAKE_CASE`.
- Use descriptive names (`send_line_reply`, `discord_create_thread`).

### Types & Ownership
- Prefer `&AppState` for read-only access; use `Arc<AppState>` when shared.
- Use `u64` for Discord IDs; parse via helper (`parse_discord_id`).
- Avoid `String` clones unless necessary; pass `&str` when possible.

### Error Handling
- Use `anyhow::Result` and `Context` for fallible ops.
- Return early on invalid inputs (guard clauses).
- Avoid `unwrap` unless invariant is guaranteed; prefer `unwrap_or_default` only when safe.
- Include HTTP status/body in error messages for API calls.

### Logging
- Use `tracing` macros (`info!`, `warn!`, `error!`).
- Log structured fields (`?err`, `event_id`, `thread_id`).
- Do not log secrets or tokens.

### HTTP / Axum
- Handlers return `impl IntoResponse` with `StatusCode` for errors.
- Validate headers and signatures before parsing payloads.
- Keep request parsing isolated and return `BAD_REQUEST` on parse failure.

### External APIs
- Reuse shared `reqwest::Client` from `AppState`.
- Check `.status().is_success()` before treating responses as OK.
- For LINE reply, treat invalid token specially (see `LineReplyOutcome`).

### Database / SQLx
- Use `sqlx::query` with `?` bind parameters (no string interpolation).
- Keep DB helpers small and return `Result`.
- Keep timestamp helpers centralized (`now_ts`).

### JSON / Serde
- Use `#[serde(rename_all = "camelCase")]` consistently for LINE payloads.
- Use `#[serde(rename = "type")]` for reserved keywords.
- Keep request/response structs near usage.

### Control Flow
- Prefer early returns to reduce nesting.
- Use `match` for fallible branching; keep error paths explicit.

### Testing
- Add unit tests under `#[cfg(test)]` modules in `src/`.
- Use `#[tokio::test]` for async tests.
- Avoid network calls in tests; mock or structure helpers.
- Prefer deterministic timestamps by injecting helpers if needed.

## Database Notes / DB
- Schema is created at startup in `init_db`.
- No migrations are present; update schema carefully if needed.
- Keep table/column names snake_case.

## Service Files / サービス
- `discord-line.service` is the systemd unit for deployment.
- `Caddyfile` likely handles reverse proxy; update only when asked.

## Cursor/Copilot Rules / 追加ルール
- No `.cursorrules` or `.cursor/rules` found in this repo.
- No `.github/copilot-instructions.md` found.

## Notes for Agents / エージェント向けメモ
- Keep changes focused; avoid refactors unless required.
- Update `Caddyfile` or `discord-line.service` only when requested.
- When touching `main.rs`, keep functions grouped by responsibility.
- Use helper functions instead of duplicating API/DB code.
- Prefer small, composable functions over large monoliths.
- For new modules, update `main.rs` with `mod` declarations.
- If you add config, update env var list here.
- When unsure, ask the user before changing runtime behavior.

## Quick Reference / 早見表
- Build: `cargo build`
- Run: `cargo run`
- Test: `cargo test`
- Single test: `cargo test <name>`
- Format: `cargo fmt`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`
