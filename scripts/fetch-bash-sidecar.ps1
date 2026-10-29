# Download MinGit for Windows (portable POSIX/MSYS environment), verify SHA-256, extract to DestDir.
# MinGit ships usr/bin/sh.exe (and dash.exe), not GNU bash.exe. After extract we copy sh.exe
# to bash.exe so Pi's `bash` lookup on PATH succeeds. Exit non-zero on any failure.
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

    $usrBin = Join-Path $DestDir "usr\bin"
    $bashExe = Join-Path $usrBin "bash.exe"
    $shExe = Join-Path $usrBin "sh.exe"
    if (-not (Test-Path -LiteralPath $bashExe)) {
        if (-not (Test-Path -LiteralPath $shExe)) {
            Write-Error "neither bash.exe nor sh.exe found under $usrBin"
            exit 1
        }
        Copy-Item -LiteralPath $shExe -Destination $bashExe
        Write-Host "MinGit has no bash.exe; copied sh.exe -> bash.exe for Pi PATH lookup"
    }

    Write-Host "Bash sidecar ready: $bashExe"
}
finally {
    if (Test-Path -LiteralPath $tmpZip) {
        Remove-Item -LiteralPath $tmpZip -Force
    }
}
