# Native Windows demonstration; requires the built Rust executable and Git 2.36+.
$ErrorActionPreference = 'Stop'
$Mai = if ($env:MAI_BIN) { $env:MAI_BIN } else { Join-Path $PSScriptRoot '../target/release/mai.exe' }
if (-not (Test-Path $Mai)) {
    cargo build --manifest-path (Join-Path $PSScriptRoot '../Cargo.toml') --release --locked --bin mai
    if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed' }
}
$Project = Join-Path ([IO.Path]::GetTempPath()) ('mai-demo-' + [guid]::NewGuid())
function Invoke-Mai {
    & $Mai @args
    if ($LASTEXITCODE -ne 0) { throw "mai failed with exit code $LASTEXITCODE" }
}
Invoke-Mai init $Project
Invoke-Mai workspace $Project frontend
Invoke-Mai workspace $Project backend
Invoke-Mai lease $Project frontend web --ttl 120
Invoke-Mai lease $Project backend api --ttl 120
$Web = Join-Path $Project 'workspaces/frontend/web'
$Api = Join-Path $Project 'workspaces/backend/api'
New-Item -ItemType Directory -Path $Web, $Api | Out-Null
[IO.File]::WriteAllText((Join-Path $Web 'index.txt'), "index page`n")
[IO.File]::WriteAllText((Join-Path $Api 'server.txt'), "api handler`n")
Invoke-Mai --json submit $Project frontend -m 'Add UI'
Invoke-Mai --json submit $Project backend -m 'Add API' --check 'if not exist api\server.txt exit /b 1'
Invoke-Mai history $Project
Write-Output "Demo retained at $Project"
