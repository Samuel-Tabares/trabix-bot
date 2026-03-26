param()

$ErrorActionPreference = "Stop"

$rootDir = Split-Path -Parent $PSScriptRoot
$defaultDbContainer = "granizado-bot-simulator-db"
$defaultDbName = "granizado_bot_local"
$defaultDbUser = "postgres"
$defaultDbPassword = "postgres"
$defaultDbPort = "5432"

if (-not $env:BOT_MODE) { $env:BOT_MODE = "simulator" }
if (-not $env:PORT) { $env:PORT = "8080" }
if (-not $env:ADVISOR_PHONE) { $env:ADVISOR_PHONE = "573001234567" }
if (-not $env:SIMULATOR_UPLOAD_DIR) { $env:SIMULATOR_UPLOAD_DIR = Join-Path $rootDir ".simulator_uploads" }

$simulatorUrl = "http://127.0.0.1:$($env:PORT)/simulator"

function Ensure-DockerDatabase {
    $container = if ($env:SIMULATOR_DB_CONTAINER) { $env:SIMULATOR_DB_CONTAINER } else { $defaultDbContainer }
    $dbName = if ($env:SIMULATOR_DB_NAME) { $env:SIMULATOR_DB_NAME } else { $defaultDbName }
    $dbUser = if ($env:SIMULATOR_DB_USER) { $env:SIMULATOR_DB_USER } else { $defaultDbUser }
    $dbPassword = if ($env:SIMULATOR_DB_PASSWORD) { $env:SIMULATOR_DB_PASSWORD } else { $defaultDbPassword }
    $dbPort = if ($env:SIMULATOR_DB_PORT) { $env:SIMULATOR_DB_PORT } else { $defaultDbPort }

    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
        throw "DATABASE_URL no está configurado y Docker no está disponible."
    }

    $existing = docker ps -a --format '{{.Names}}' | Where-Object { $_ -eq $container }
    if (-not $existing) {
        Write-Host "Creando contenedor Postgres local '$container'..."
        docker run --name $container -e "POSTGRES_PASSWORD=$dbPassword" -e "POSTGRES_DB=$dbName" -p "${dbPort}:5432" -d postgres:16 | Out-Null
    } else {
        $running = docker inspect -f '{{.State.Running}}' $container 2>$null
        if ($running -ne "true") {
            Write-Host "Iniciando contenedor Postgres local '$container'..."
            docker start $container | Out-Null
        }
    }

    $env:DATABASE_URL = "postgresql://${dbUser}:${dbPassword}@localhost:${dbPort}/${dbName}"
}

New-Item -ItemType Directory -Force -Path $env:SIMULATOR_UPLOAD_DIR | Out-Null

$menuAssetPath = Join-Path $rootDir "assets/menu-placeholder.svg"
if (-not (Test-Path $menuAssetPath)) {
    throw "No existe el menú fallback del simulador: $menuAssetPath"
}

if (-not $env:DATABASE_URL) {
    Ensure-DockerDatabase
}

Write-Host "Lanzando simulator en $simulatorUrl"
Write-Host "DATABASE_URL=$($env:DATABASE_URL)"
Write-Host "SIMULATOR_MENU_ASSET=$menuAssetPath"

Start-Job -ScriptBlock {
    param($url)
    Start-Sleep -Seconds 4
    Start-Process $url
} -ArgumentList $simulatorUrl | Out-Null

Push-Location $rootDir
try {
    cargo run --bin granizado-bot
} finally {
    Pop-Location
}
