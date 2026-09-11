# Eggress standalone CLI installer (Windows).
#
# Installs prebuilt `eggress.exe` and `pproxy.exe` from a GitHub Release as one
# version-aligned unit. Release binaries use the default `eggress-cli` feature
# set; custom features (ssh, quic, legacy crypto, pproxy legacy) require a
# Cargo/source build.
#
# Auditable use (preferred over piping remote code to the shell):
#   Invoke-WebRequest -Uri https://github.com/eggstack/eggress/releases/latest/download/install.ps1 -OutFile install.ps1
#   Get-Content install.ps1   # review before running
#   powershell -ExecutionPolicy Bypass -File install.ps1
#
# One-liner (convenient but runs remote code; review the script above first):
#   irm https://github.com/eggstack/eggress/releases/latest/download/install.ps1 | iex
#
# Integrity note: the SHA-256 sidecar is downloaded from the same GitHub
# Release as the archive. It detects corruption or mismatched assets; it is
# not an independent signature or publisher-authentication guarantee.
#
# Test-only override: set $env:EGRESS_RELEASE_BASE_URL to a base URL (for
# example file:///C:/fixture-release) to resolve assets from local fixtures or
# a mock endpoint instead of github.com. Not for production use.

param(
  [string]$Version = "",
  [string]$InstallDir = ""
)

$ErrorActionPreference = "Stop"

$Repo = "eggstack/eggress"
$DefaultBaseUrl = "https://github.com/$Repo/releases"
$Target = "x86_64-pc-windows-msvc"
$Archive = "eggress-$Target.zip"
$ChecksumFile = "$Archive.sha256"

if ($Version -ne "" -and $Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') {
  throw "Version must be X.Y.Z (e.g. -Version 1.2.3), got '$Version'"
}

# Only Windows x86_64 has prebuilt binaries in the initial matrix.
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne "AMD64") {
  throw "No prebuilt Eggress release for Windows/$arch. Install with Cargo instead: cargo install eggress-cli --locked. See docs/INSTALLATION.md for source builds."
}

$BaseUrl = $env:EGRESS_RELEASE_BASE_URL
if ([string]::IsNullOrEmpty($BaseUrl)) { $BaseUrl = $DefaultBaseUrl }
if ($Version -ne "") {
  $Tag = "v$Version"
  $ReleaseUrl = "$BaseUrl/download/$Tag"
} else {
  $Tag = "latest"
  $ReleaseUrl = "$BaseUrl/latest/download"
}

if ([string]::IsNullOrEmpty($InstallDir)) {
  # User-writable default: no Administrator rights required. For a
  # system-wide location, rerun from an elevated shell with -InstallDir.
  $InstallDir = Join-Path $env:USERPROFILE ".local\bin"
}

if (-not (Test-Path $InstallDir)) {
  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}
# Fail before mutating either binary when the destination is not writable.
try {
  $probe = Join-Path $InstallDir ".eggress-write-test"
  [System.IO.File]::WriteAllText($probe, "write-test")
  Remove-Item $probe -Force
} catch {
  throw "Install directory '$InstallDir' is not writable. Rerun from a shell with write access or choose a user-writable directory, e.g. powershell -File install.ps1 -InstallDir `"$env:USERPROFILE\.local\bin`""
}

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("eggress-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
try {
  Write-Output "Downloading $Archive ($Tag) for $Target"
  Invoke-WebRequest -Uri "$ReleaseUrl/$Archive" -OutFile (Join-Path $TempDir $Archive) -UseBasicParsing
  Invoke-WebRequest -Uri "$ReleaseUrl/$ChecksumFile" -OutFile (Join-Path $TempDir $ChecksumFile) -UseBasicParsing

  $expected = ((Get-Content (Join-Path $TempDir $ChecksumFile) -TotalCount 1) -split '\s+')[0]
  if ([string]::IsNullOrEmpty($expected) -or $expected -notmatch '^[0-9a-fA-F]{64}$') {
    throw "Checksum file is empty or malformed: $ChecksumFile"
  }
  $actual = (Get-FileHash (Join-Path $TempDir $Archive) -Algorithm SHA256).Hash.ToLower()
  if ($actual -ne $expected.ToLower()) {
    throw "SHA-256 mismatch for $Archive (expected $expected, actual $actual)"
  }
  Write-Output "Checksum verified: $Archive"

  Expand-Archive -Path (Join-Path $TempDir $Archive) -DestinationPath (Join-Path $TempDir "stage") -Force
  $stagedEggress = Join-Path $TempDir "stage\eggress.exe"
  $stagedPproxy = Join-Path $TempDir "stage\pproxy.exe"
  if (-not (Test-Path $stagedEggress)) { throw "Archive is missing eggress.exe" }
  if (-not (Test-Path $stagedPproxy)) { throw "Archive is missing pproxy.exe" }

  # Verify both staged executables before installing either one.
  $eggressOut = & $stagedEggress version
  if ($eggressOut -notmatch '^eggress ([0-9]+\.[0-9]+\.[0-9]+)$') {
    throw "Staged eggress version mismatch: '$eggressOut'"
  }
  $stagedVersion = $Matches[1]
  $pproxyOut = & $stagedPproxy --version
  $m = [regex]::Match($pproxyOut, '[0-9]+\.[0-9]+\.[0-9]+')
  if (-not $m.Success) { throw "Staged pproxy version mismatch: '$pproxyOut'" }
  $pproxyVersion = $m.Value
  if ($stagedVersion -ne $pproxyVersion) {
    throw "Staged binary versions disagree: eggress $stagedVersion vs pproxy $pproxyVersion"
  }
  if ($Version -ne "" -and $stagedVersion -ne $Version) {
    throw "Staged version $stagedVersion != requested version $Version"
  }
  Write-Output "Staged versions verified: eggress $stagedVersion / pproxy $pproxyVersion"

  Copy-Item $stagedEggress (Join-Path $InstallDir "eggress.exe") -Force
  Copy-Item $stagedPproxy (Join-Path $InstallDir "pproxy.exe") -Force

  Write-Output "Installed eggress $stagedVersion to $(Join-Path $InstallDir 'eggress.exe')"
  Write-Output "Installed pproxy $pproxyVersion to $(Join-Path $InstallDir 'pproxy.exe')"

  # Never mutate PATH registry/profile state automatically; advise instead.
  $pathEntries = ($env:PATH -split ';') | ForEach-Object { $_.TrimEnd('\') }
  if ($pathEntries -notcontains $InstallDir.TrimEnd('\')) {
    Write-Warning "$InstallDir is not in PATH. Add it to use eggress without a full path, e.g.: `$env:PATH += ';$InstallDir'`"
  }
} finally {
  Remove-Item $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
