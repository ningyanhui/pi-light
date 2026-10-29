# Download MinGit for Windows (portable POSIX/MSYS environment), verify SHA-256, extract to DestDir.
#
# MinGit-2.47.1.2-64-bit.zip layout (no extra top-level folder):
#   usr/bin/sh.exe      POSIX shell (present)
#   usr/bin/dash.exe    dash (present)
#   usr/bin/bash.exe    NOT in the zip
#   mingw64/share/git/edit-git-bash.exe  is a helper, not bash
#
# After extract we search DestDir for bash.exe; if none, copy usr/bin/sh.exe to
# usr/bin/bash.exe so Pi's PATH lookup of `bash` succeeds. SHA check is unchanged.
# Exit non-zero on any failure; never packages an unverified zip.
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

function Find-NamedExe {
    param(
        [Parameter(Mandatory = $true)][string] $Root,
        [Parameter(Mandatory = $true)][string] $FileName
    )
    if (-not (Test-Path -LiteralPath $Root)) { return $null }
    Get-ChildItem -LiteralPath $Root -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -ieq $FileName } |
        Select-Object -First 1
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

    # Locate a usable shell, then ensure bash.exe sits next to it (Pi looks up `bash`).
    $foundBash = Find-NamedExe -Root $DestDir -FileName "bash.exe"
    if ($null -ne $foundBash) {
        Write-Host "Found bash.exe at $($foundBash.FullName)"
        $canonicalBash = $foundBash.FullName
    }
    else {
        Write-Host "No bash.exe in MinGit extract (expected for MinGit-2.47.1.2); looking for sh.exe ..."
        $foundSh = Find-NamedExe -Root $DestDir -FileName "sh.exe"
        if ($null -eq $foundSh) {
            Write-Error "MinGit extract has neither bash.exe nor sh.exe under $DestDir"
            exit 1
        }
        Write-Host "Real shell path: $($foundSh.FullName)"
        $canonicalBash = Join-Path $foundSh.DirectoryName "bash.exe"
        Copy-Item -LiteralPath $foundSh.FullName -Destination $canonicalBash -Force
        Write-Host "Copied sh.exe -> $canonicalBash (Pi looks up bash on PATH)"
    }

    if (-not (Test-Path -LiteralPath $canonicalBash)) {
        Write-Error "bash.exe missing after locate/copy: $canonicalBash"
        exit 1
    }

    Write-Host "Bash sidecar ready: $canonicalBash"
}
finally {
    if (Test-Path -LiteralPath $tmpZip) {
        Remove-Item -LiteralPath $tmpZip -Force
    }
}
