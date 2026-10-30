# Download official cloudflared for Windows amd64, verify SHA-256, copy to DestDir.
# Exit non-zero on failure. Binary is not committed to git.
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string] $DestDir,

    [string] $LockFile = ""
)

$ErrorActionPreference = "Stop"

if (-not $LockFile) {
    $LockFile = Join-Path $PSScriptRoot "cloudflared.lock.json"
}

if (-not (Test-Path -LiteralPath $LockFile)) {
    Write-Error "lock file missing: $LockFile"
    exit 1
}

$lock = Get-Content -LiteralPath $LockFile -Raw | ConvertFrom-Json
$art = $lock.artifacts.'windows-amd64'
if (-not $art) {
    Write-Error "windows-amd64 artifact missing from lock file"
    exit 1
}

$url = [string]$art.url
$expected = ([string]$art.sha256).ToLowerInvariant()
$installName = [string]$art.install_name
if (-not $installName) { $installName = "cloudflared.exe" }

if (-not (Test-Path -LiteralPath $DestDir)) {
    New-Item -ItemType Directory -Path $DestDir -Force | Out-Null
}

$tmp = Join-Path $env:TEMP ("cloudflared-" + [guid]::NewGuid().ToString() + ".exe")
try {
    Write-Host "Downloading cloudflared $($lock.version) from $url ..."
    Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing

    $hash = (Get-FileHash -LiteralPath $tmp -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $expected) {
        Write-Error "SHA-256 mismatch: expected $expected got $hash"
        exit 1
    }

    $dest = Join-Path $DestDir $installName
    Copy-Item -LiteralPath $tmp -Destination $dest -Force
    Write-Host "cloudflared ready: $dest"
}
finally {
    if (Test-Path -LiteralPath $tmp) {
        Remove-Item -LiteralPath $tmp -Force
    }
}
