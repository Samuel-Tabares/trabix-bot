# Trabix Granizados Bot

Trabix is a real Rust WhatsApp ordering bot for granizados, located on Armenia, Quindío, Colombia. The repository now includes two ways to run it:

- `production`: the real Meta/WhatsApp webhook runtime
- `simulator`: a local web chat that uses the same bot brain without sending anything to Meta

The simulator is meant for manual testing of the real flow on your own computer. It reuses the same core runtime for:

- conversation state machine
- PostgreSQL persistence
- order and order-item storage
- advisor flow
- timers and timeout recovery
- pricing and checkout behavior

It is not a fake demo bot. It is a local transport over the real bot logic.

## What The Simulator Gives You

When you run `BOT_MODE=simulator`, the app serves a local chat UI at:

`http://127.0.0.1:8080/simulator`

From there you can:

- create simulated customers with phone and profile name
- interact with the same production bot brain locally
- test advisor flows and timeouts
- upload receipt images locally
- inspect persisted conversation and order state
- inspect raw `conversations`, `orders`, and `order_items` rows from the simulator UI
- validate restarts and timer recovery without touching Meta

The simulator frontend is now split into editable assets:

- `assets/simulator/index.html`
- `assets/simulator/simulator.css`
- `assets/simulator/simulator.js`

The simulator backend HTTP handlers live in `src/simulator/web.rs`, while `src/routes/simulator.rs` stays as a thin mount wrapper.

## What It Does Not Replace

The simulator does not reproduce Meta platform behavior itself. It does not test:

- real webhook delivery from Meta
- WhatsApp read/status callbacks
- Meta signature validation
- Cloud API transport failures or formatting quirks

For those cases you still need real WhatsApp validation.

## Quickstart

### Requirements

- Rust and Cargo
- PostgreSQL

If you do not already have PostgreSQL running, the provided launcher can start a local Docker Postgres container for you when Docker is available.

### macOS / Linux

```bash
./scripts/run_simulator.sh
```

### Windows

```powershell
scripts\run_simulator.ps1
```

or:

```bat
scripts\run_simulator.bat
```

Then open:

`http://127.0.0.1:8080/simulator`

## Manual Startup

If you want to run it yourself:

```bash
BOT_MODE=simulator \
DATABASE_URL=postgresql://postgres:postgres@localhost:5432/granizado_bot_local \
ADVISOR_PHONE=573001234567 \
cargo run --bin granizado-bot
```

Important:

- if `BOT_MODE` is omitted, the app defaults to `production`
- simulator mode should use a local database only
- simulator mode must never point at production credentials or a production database

## Shared Bot Brain

The simulator follows production behavior for shared logic. If you change shared runtime areas later, the simulator should reflect that too:

- state machine transitions
- timers
- pricing
- order flow
- advisor flow
- persistence

Meta-specific transport behavior can still differ by design.

## Local Data And Files

Simulator test data stays local when you use a local database:

- conversations, orders, and simulator transcript rows stay in your local Postgres
- uploaded simulator images go to `.simulator_uploads/`
- nothing should be sent to Meta while in simulator mode

Pushing the repository does not push your local simulator conversation data, because that data is stored in your database, not in Git.

## Simulator Menu Asset

The simulator always uses this tracked menu image for `Ver Menú`:

`assets/trabix-menu.png`

If you want a different shared simulator menu, replace that file and commit it.

## Production Safety

Production mode is still the default when `BOT_MODE` is not set. In normal deployment:

- `/webhook` is used in production
- `/simulator` is used only in simulator mode

The simulator should not activate in Railway unless `BOT_MODE=simulator` is explicitly set.

## License

This repository is distributed under proprietary `All Rights Reserved` terms. Public visibility on GitHub does not grant permission to copy, modify, redistribute, or sell the project.
