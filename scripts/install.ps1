<#
.SYNOPSIS
    Grava o FLOWPAUSE_CLIENT_ID do usuário, compila e instala o flowpause.
.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install.ps1
.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -ClientId 0123456789abcdef0123456789abcdef
#>
param(
    [string]$ClientId
)

function Fail([string]$Message) {
    Write-Host "ERRO: $Message" -ForegroundColor Red
    exit 1
}

$repo = Split-Path -Parent $PSScriptRoot
$targetDir = Join-Path $repo 'target'
$installedExe = Join-Path $env:USERPROFILE '.cargo\bin\flowpause.exe'

Write-Host @"

ANTES DE CONTINUAR: Kaspersky
A compilação gera executáveis temporários (build scripts) em target\ e instala
flowpause.exe em .cargo\bin. O Kaspersky já pôs um desses arquivos em quarentena
e a compilação falhou com "Acesso negado" / "arquivo em uso".

Faça uma destas opções:
  1. Adicione exclusões no Kaspersky para:
       $targetDir
       $installedExe
     (em Configurações > Configurações de segurança > Exclusões; o nome exato
     do menu muda entre versões do Kaspersky)
  2. Ou pause a proteção do Kaspersky até o fim deste script.

Se algo já foi para a quarentena, restaure e marque como confiável antes.

"@ -ForegroundColor Yellow
Read-Host 'Pressione Enter para continuar ou Ctrl+C para cancelar' | Out-Null

# Client ID: parâmetro, depois variável de ambiente, depois pergunta.
if (-not $ClientId) { $ClientId = $env:FLOWPAUSE_CLIENT_ID }
if (-not $ClientId) { $ClientId = Read-Host 'Client ID do app Spotify (veja docs\CLIENT-ID.md)' }
$ClientId = "$ClientId".Trim()
if (-not $ClientId) { Fail 'Client ID vazio. Veja docs\CLIENT-ID.md para saber onde pegar.' }
if ($ClientId -notmatch '^[0-9a-fA-F]{32}$') {
    Write-Warning "O Client ID costuma ter 32 caracteres hexadecimais; '$ClientId' não tem esse formato. Confira se copiou o Client ID, e não o Client Secret."
}

[Environment]::SetEnvironmentVariable('FLOWPAUSE_CLIENT_ID', $ClientId, 'User')
$env:FLOWPAUSE_CLIENT_ID = $ClientId
Write-Host "FLOWPAUSE_CLIENT_ID gravado nas variáveis do usuário."

$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
if (-not (Test-Path $cargo)) { Fail 'cargo não encontrado. Instale o Rust (https://rustup.rs) com o toolchain MSVC.' }

$avHint = 'Se a mensagem citar "Acesso negado" ou "arquivo já está sendo usado", é o Kaspersky: confira a quarentena e as exclusões acima.'

Push-Location $repo
try {
    Write-Host "`n> cargo build --release" -ForegroundColor Cyan
    & $cargo build --release
    if ($LASTEXITCODE -ne 0) { Fail "cargo build falhou (código $LASTEXITCODE). $avHint" }

    Write-Host "`n> cargo install --path . --locked" -ForegroundColor Cyan
    & $cargo install --path . --locked
    if ($LASTEXITCODE -ne 0) { Fail "cargo install falhou (código $LASTEXITCODE). $avHint" }
}
finally {
    Pop-Location
}

if (-not (Test-Path $installedExe)) {
    Fail "flowpause.exe não está em $installedExe. O Kaspersky provavelmente o pôs em quarentena."
}
& $installedExe --version
if ($LASTEXITCODE -ne 0) { Fail "flowpause.exe não executou. $avHint" }

Write-Host @"

Pronto. Abra um terminal novo (para carregar FLOWPAUSE_CLIENT_ID) e rode:
  flowpause login
Depois siga o roteiro de teste do README.
"@ -ForegroundColor Green
