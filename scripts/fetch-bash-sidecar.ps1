# Download MinGit for Windows (portable bash/MSYS environment), verify SHA-256, extract to DestDir.
# MinGit bundles bash.exe + MSYS2 runtime without requiring a full Git installation.
# Exit non-zero on any failure; never packages an unverified binary.
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string] $DestDir,

    [string] $MinGitUrl = "https://github.com/git-for-windows/git/releases/download/v2.47.1.windows.2/MinGit-2.47.1.2-64-bit.zip",
    [string] $ExpectedSha256 = "5bafb35dfb249b89d726b37824eeb5022379f0e51f5fbf9c29f49bef57e85b42"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $DestDir)) {
    New-Item -ItemType Directory -Path $DestDir -Force | Out-Null
}

$tmpZip = Join-Path $env:TEMP ("mingit-" + [guid]::NewGuid().ToString() + ".zip")
try {
    Write-Host "Downloading MinGit from $MinGitUrl ..."
    Invoke-WebRequest -Uri $MinGitUrl -OutFile $tmpZip -UseBasicParsing

    $hash = (Get-FileHash -LiteralPath $tmpZip -Algorithm SHA256).Hash.ToLowerInvariant()
    $expected = $ExpectedSha256.ToLowerInvariant()
    if ($hash -ne $expected) {
        Write-Error "SHA-256 mismatch for MinGit: expected $expected, got $hash"
        exit 1
    }
    Write-Host "SHA-256 verified: $hash"

    Write-Host "Extracting MinGit into $DestDir ..."
    Expand-Archive -LiteralPath $tmpZip -DestinationPath $DestDir -Force

    # Verify that bash.exe landed where expected.
    $bashExe = Join-Path $DestDir "usr\bin\bash.exe"
    if (-not (Test-Path -LiteralPath $bashExe)) {
        Write-Error "bash.exe not found at expected path: $bashExe"
        exit 1
    }

    Write-Host "Bash sidecar ready: $bashExe"
}
finally {
    if (Test-Path -LiteralPath $tmpZip) {
        Remove-Item -LiteralPath $tmpZip -Force
    }
}
