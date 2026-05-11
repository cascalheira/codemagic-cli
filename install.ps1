# install.ps1 — installs the latest codemagic-cli release on Windows.
#
# Usage (run in PowerShell as Administrator, or with a writable InstallDir):
#
#   # Latest release, default install directory (%LOCALAPPDATA%\codemagic-cli):
#   irm https://raw.githubusercontent.com/cascalheira/codemagic-cli/main/install.ps1 | iex
#
#   # Specify a custom install directory:
#   $env:INSTALL_DIR = "C:\Tools"; irm https://raw.githubusercontent.com/cascalheira/codemagic-cli/main/install.ps1 | iex
#
#   # Pin a specific version:
#   $env:VERSION = "v1.3.0"; irm https://raw.githubusercontent.com/cascalheira/codemagic-cli/main/install.ps1 | iex
#
# Environment variables (all optional):
#   INSTALL_DIR  — directory to install the binary (default: %LOCALAPPDATA%\codemagic-cli)
#   VERSION      — specific release tag to install  (default: latest)

[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$Repo       = "cascalheira/codemagic-cli"
$BinaryName = "codemagic-cli.exe"

# ── Resolve install directory ────────────────────────────────────────────────
$InstallDir = if ($env:INSTALL_DIR) {
    $env:INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA "codemagic-cli"
}

# ── Detect architecture ───────────────────────────────────────────────────────
$Arch = $env:PROCESSOR_ARCHITECTURE
$ArchName = switch ($Arch) {
    "AMD64" { "x86_64"  }
    "ARM64" { "aarch64" }
    default {
        Write-Error "Unsupported architecture: $Arch"
        exit 1
    }
}

$AssetName = "codemagic-cli-windows-${ArchName}.zip"

# ── Resolve version ───────────────────────────────────────────────────────────
$Version = $env:VERSION
if (-not $Version) {
    Write-Host "Fetching latest release info from GitHub..."
    # Use the GitHub API — returns JSON with a "tag_name" field.
    $ApiUrl = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $Json = Invoke-WebRequest `
            -Uri $ApiUrl `
            -UseBasicParsing `
            -Headers @{ "User-Agent" = "codemagic-cli-installer"; "Accept" = "application/json" } `
            | Select-Object -ExpandProperty Content
        $Version = ($Json | ConvertFrom-Json).tag_name
    } catch {
        $ErrMsg = $_.Exception.Message
        Write-Host "ERROR: Could not fetch release info from $ApiUrl" -ForegroundColor Red
        Write-Host "       $ErrMsg" -ForegroundColor Red
        exit 1
    }
    if (-not $Version) {
        Write-Host "ERROR: GitHub API returned an empty tag_name. Response was:" -ForegroundColor Red
        Write-Host $Json -ForegroundColor Red
        exit 1
    }
}

Write-Host "Installing codemagic-cli $Version for windows/$ArchName..."

$BaseUrl     = "https://github.com/$Repo/releases/download/$Version"
$DownloadUrl = "$BaseUrl/$AssetName"
$ChecksumUrl = "$DownloadUrl.sha256"

# ── Download to a temp directory ──────────────────────────────────────────────
$TmpDir = Join-Path $env:TEMP "codemagic-cli-install-$(Get-Random)"
New-Item -ItemType Directory -Path $TmpDir | Out-Null

try {
    $ZipPath      = Join-Path $TmpDir $AssetName
    $ChecksumPath = Join-Path $TmpDir "$AssetName.sha256"

    Write-Host "Downloading $AssetName..."
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath -UseBasicParsing

    # ── Verify checksum ───────────────────────────────────────────────────────
    Write-Host "Verifying checksum..."
    Invoke-WebRequest -Uri $ChecksumUrl -OutFile $ChecksumPath -UseBasicParsing

    # .sha256 file format: "<hash>  <filename>"
    $ExpectedLine = Get-Content $ChecksumPath -Raw
    $ExpectedHash = ($ExpectedLine -split '\s+')[0].Trim().ToUpper()
    $ActualHash   = (Get-FileHash -Algorithm SHA256 -Path $ZipPath).Hash.ToUpper()

    if ($ExpectedHash -ne $ActualHash) {
        Write-Error "Checksum mismatch!`n  Expected : $ExpectedHash`n  Got      : $ActualHash"
        exit 1
    }
    Write-Host "Checksum OK."

    # ── Extract ────────────────────────────────────────────────────────────────
    Write-Host "Extracting..."
    Expand-Archive -Path $ZipPath -DestinationPath $TmpDir -Force

    # ── Install ────────────────────────────────────────────────────────────────
    if (-not (Test-Path $InstallDir)) {
        Write-Host "Creating $InstallDir..."
        New-Item -ItemType Directory -Path $InstallDir | Out-Null
    }

    $Destination = Join-Path $InstallDir $BinaryName
    Move-Item -Path (Join-Path $TmpDir $BinaryName) -Destination $Destination -Force

    Write-Host ""
    Write-Host "✓  codemagic-cli $Version installed to $Destination"
    Write-Host ""

    # ── Add to PATH (user scope) if not already present ────────────────────────
    $UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable(
            "PATH",
            "$UserPath;$InstallDir",
            "User"
        )
        Write-Host "Added $InstallDir to your user PATH."
        Write-Host "Restart your terminal (or open a new one) for the change to take effect."
    } else {
        Write-Host "Run 'codemagic-cli' to get started."
    }

} finally {
    # Clean up temp directory
    if (Test-Path $TmpDir) {
        Remove-Item -Recurse -Force $TmpDir
    }
}
