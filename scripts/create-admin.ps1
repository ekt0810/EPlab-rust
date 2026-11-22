$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$binary = Join-Path $projectRoot 'target/release/eplab-rust.exe'
if (-not (Test-Path -LiteralPath $binary)) { throw 'Run cargo build --release first.' }
$env:ADMIN_EMAIL = Read-Host 'Email nguoi chu tri'
$env:ADMIN_NAME = Read-Host 'Ho va ten'
$privatePassword = Read-Host 'Mat khau (12-128 byte)' -AsSecureString
$privatePointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($privatePassword)
try {
    $env:ADMIN_PASSWORD = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($privatePointer)
    Push-Location $projectRoot
    try {
        & $binary admin
        if ($LASTEXITCODE -ne 0) { throw 'Administrator creation failed.' }
    } finally { Pop-Location }
} finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($privatePointer)
    Remove-Item Env:ADMIN_PASSWORD -ErrorAction SilentlyContinue
    Remove-Item Env:ADMIN_EMAIL -ErrorAction SilentlyContinue
    Remove-Item Env:ADMIN_NAME -ErrorAction SilentlyContinue
}
