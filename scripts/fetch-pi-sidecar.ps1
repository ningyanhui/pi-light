# Download official Pi v1.4.0 Windows zip, verify SHA-256, extract to DestDir.
# Exit non-zero on failure.
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string] $DestDir,

    [string] $PiUrl = "https://github.com/earendil-works/pi/releases/download/v1.4.0/pi-windows-x64.zip",
    [string] $ExpectedSha256 = "0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $DestDir)) {
    New-Item -ItemType Directory -Path $DestDir -Force | Out-Null
}

$tmpZip = Join-Path $env:TEMP ("pi-windows-x64-" + [guid]::NewGuid().ToString() + ".zip")
try {
    Write-Host "Downloading Pi sidecar from $PiUrl ..."
    Invoke-WebRequest -Uri $PiUrl -OutFile $tmpZip -UseBasicParsing

    $hash = (Get-FileHash -LiteralPath $tmpZip -Algorithm SHA256).Hash.ToLowerInvariant()
    $expected = $ExpectedSha256.ToLowerInvariant()
    if ($hash -ne $expected) {
        Write-Error "SHA-256 mismatch: expected $expected got $hash"
        exit 1
    }

    Write-Host "Extracting into $DestDir ..."
    Expand-Archive -LiteralPath $tmpZip -DestinationPath $DestDir -Force

    $piExe = Join-Path $DestDir "pi.exe"
    if (-not (Test-Path -LiteralPath $piExe)) {
        Write-Error "pi.exe not found under $DestDir after extract"
        exit 1
    }

    Write-Host "Pi sidecar ready: $piExe"
}
finally {
    if (Test-Path -LiteralPath $tmpZip) {
        Remove-Item -LiteralPath $tmpZip -Force
    }
}
