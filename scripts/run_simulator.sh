#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEFAULT_DB_CONTAINER="granizado-bot-simulator-db"
DEFAULT_DB_NAME="granizado_bot_local"
DEFAULT_DB_USER="postgres"
DEFAULT_DB_PASSWORD="postgres"
DEFAULT_DB_PORT="5432"
SIMULATOR_URL="http://127.0.0.1:${PORT:-8080}/simulator"

export BOT_MODE="simulator"
export PORT="${PORT:-8080}"
export ADVISOR_PHONE="${ADVISOR_PHONE:-573001234567}"
export SIMULATOR_UPLOAD_DIR="${SIMULATOR_UPLOAD_DIR:-$ROOT_DIR/.simulator_uploads}"

ensure_docker_database() {
  local container="${SIMULATOR_DB_CONTAINER:-$DEFAULT_DB_CONTAINER}"
  local db_name="${SIMULATOR_DB_NAME:-$DEFAULT_DB_NAME}"
  local db_user="${SIMULATOR_DB_USER:-$DEFAULT_DB_USER}"
  local db_password="${SIMULATOR_DB_PASSWORD:-$DEFAULT_DB_PASSWORD}"
  local db_port="${SIMULATOR_DB_PORT:-$DEFAULT_DB_PORT}"

  if ! command -v docker >/dev/null 2>&1; then
    echo "DATABASE_URL no está configurado y Docker no está disponible."
    echo "Configura DATABASE_URL manualmente o instala Docker para auto-crear Postgres local."
    exit 1
  fi

  if ! docker ps -a --format '{{.Names}}' | grep -Fxq "$container"; then
    echo "Creando contenedor Postgres local '$container'..."
    docker run \
      --name "$container" \
      -e POSTGRES_PASSWORD="$db_password" \
      -e POSTGRES_DB="$db_name" \
      -p "${db_port}:5432" \
      -d postgres:16 >/dev/null
  else
    local running
    running="$(docker inspect -f '{{.State.Running}}' "$container" 2>/dev/null || true)"
    if [[ "$running" != "true" ]]; then
      echo "Iniciando contenedor Postgres local '$container'..."
      docker start "$container" >/dev/null
    fi
  fi

  export DATABASE_URL="postgresql://${db_user}:${db_password}@localhost:${db_port}/${db_name}"
}

open_browser() {
  (
    sleep 4
    if command -v open >/dev/null 2>&1; then
      open "$SIMULATOR_URL" >/dev/null 2>&1 || true
    elif command -v xdg-open >/dev/null 2>&1; then
      xdg-open "$SIMULATOR_URL" >/dev/null 2>&1 || true
    elif command -v gio >/dev/null 2>&1; then
      gio open "$SIMULATOR_URL" >/dev/null 2>&1 || true
    fi
  ) &
}

mkdir -p "$SIMULATOR_UPLOAD_DIR"

if [[ ! -f "$ROOT_DIR/assets/menu-placeholder.svg" ]]; then
  echo "No existe el menú fallback del simulador:"
  echo "  $ROOT_DIR/assets/menu-placeholder.svg"
  exit 1
fi

if [[ -z "${DATABASE_URL:-}" ]]; then
  ensure_docker_database
fi

echo "Lanzando simulator en $SIMULATOR_URL"
echo "DATABASE_URL=$DATABASE_URL"
echo "SIMULATOR_MENU_ASSET=$ROOT_DIR/assets/menu-placeholder.svg"

cd "$ROOT_DIR"
open_browser
cargo run --bin granizado-bot
