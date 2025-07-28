# SNP Installation Script for Windows PowerShell
# This script downloads and installs the latest release of SNP

param(
    [string]$Version = "",
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\SNP",
    [switch]$Force,
    [switch]$Help
)

# Configuration
$Repo = "devops247-online/snp"
$BinaryName = "snp.exe"
$TempDir = [System.IO.Path]::GetTempPath() + [System.Guid]::NewGuid()

# Error handling
$ErrorActionPreference = "Stop"

function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Color = "White"
    )

    $originalColor = $Host.UI.RawUI.ForegroundColor
    $Host.UI.RawUI.ForegroundColor = $Color
    Write-Output $Message
    $Host.UI.RawUI.ForegroundColor = $originalColor
}

function Write-Info {
    param([string]$Message)
    Write-ColorOutput "[INFO] $Message" "Cyan"
}

function Write-Success {
    param([string]$Message)
    Write-ColorOutput "[SUCCESS] $Message" "Green"
}

function Write-Warning {
    param([string]$Message)
    Write-ColorOutput "[WARNING] $Message" "Yellow"
}

function Write-Error {
    param([string]$Message)
    Write-ColorOutput "[ERROR] $Message" "Red"
}

function Show-Usage {
    @"
SNP Installation Script for Windows

Usage: .\install.ps1 [options]

Options:
    -Version VERSION     Install specific version (e.g., v1.0.0)
    -InstallDir DIR      Installation directory (default: $env:LOCALAPPDATA\Programs\SNP)
    -Force               Overwrite existing installation
    -Help                Show this help message

Examples:
    .\install.ps1                                    # Install latest version
    .\install.ps1 -Version v1.0.0                   # Install specific version
    .\install.ps1 -InstallDir "C:\Tools\SNP"        # Install to custom directory

"@
}

function Get-LatestVersion {
    Write-Info "Fetching latest release information..."
    try {
        $response = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
        return $response.tag_name
    }
    catch {
        Write-Error "Failed to fetch latest release information: $_"
        exit 1
    }
}

function Download-Binary {
    param(
        [string]$Version,
        [string]$Platform = "x86_64-pc-windows-msvc"
    )

    $ArchiveName = "snp-$Platform.zip"
    $DownloadUrl = "https://github.com/$Repo/releases/download/$Version/$ArchiveName"
    $ChecksumUrl = "https://github.com/$Repo/releases/download/$Version/$ArchiveName.sha256"

    Write-Info "Downloading SNP $Version for Windows..."

    # Create temp directory
    New-Item -Path $TempDir -ItemType Directory -Force | Out-Null

    try {
        # Download binary archive
        $ArchivePath = Join-Path $TempDir $ArchiveName
        Invoke-WebRequest -Uri $DownloadUrl -OutFile $ArchivePath

        # Download checksum
        $ChecksumPath = Join-Path $TempDir "$ArchiveName.sha256"
        try {
            Invoke-WebRequest -Uri $ChecksumUrl -OutFile $ChecksumPath

            # Verify checksum
            Write-Info "Verifying checksum..."
            $ExpectedHash = (Get-Content $ChecksumPath).Split(' ')[0].ToUpper()
            $ActualHash = (Get-FileHash $ArchivePath -Algorithm SHA256).Hash

            if ($ExpectedHash -eq $ActualHash) {
                Write-Success "Checksum verification passed"
            }
            else {
                Write-Error "Checksum verification failed!"
                exit 1
            }
        }
        catch {
            Write-Warning "Could not download or verify checksum, proceeding without verification"
        }

        # Extract binary
        Write-Info "Extracting binary..."
        Expand-Archive -Path $ArchivePath -DestinationPath $TempDir -Force

        $BinaryPath = Join-Path $TempDir $BinaryName
        if (-not (Test-Path $BinaryPath)) {
            Write-Error "Binary not found in archive"
            exit 1
        }

        return $BinaryPath
    }
    catch {
        Write-Error "Failed to download or extract binary: $_"
        exit 1
    }
}

function Install-Binary {
    param([string]$BinaryPath)

    $InstallPath = Join-Path $InstallDir $BinaryName

    # Create installation directory
    if (-not (Test-Path $InstallDir)) {
        Write-Info "Creating installation directory: $InstallDir"
        New-Item -Path $InstallDir -ItemType Directory -Force | Out-Null
    }

    # Check if binary already exists
    if ((Test-Path $InstallPath) -and -not $Force) {
        Write-Error "SNP is already installed at $InstallPath"
        Write-Error "Use -Force to overwrite or uninstall first"
        exit 1
    }

    # Install binary
    Write-Info "Installing SNP to $InstallPath..."
    Copy-Item $BinaryPath $InstallPath -Force

    Write-Success "SNP installed successfully!"

    # Verify installation
    try {
        $Version = & $InstallPath --version 2>$null
        Write-Success "Installation verified: $Version"
    }
    catch {
        Write-Warning "Installation may have issues - binary doesn't run correctly"
    }

    # Check if install directory is in PATH
    $PathDirs = $env:PATH -split ';'
    if ($InstallDir -notin $PathDirs) {
        Write-Warning "Installation directory is not in your PATH"
        Write-Info "Add it to your PATH or run SNP with full path: $InstallPath"
        Write-Info "To add to PATH permanently, run as Administrator:"
        Write-Info "[Environment]::SetEnvironmentVariable('PATH', `$env:PATH + ';$InstallDir', 'Machine')"
    }
}

function Remove-TempDirectory {
    if (Test-Path $TempDir) {
        Remove-Item $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Main {
    # Show help if requested
    if ($Help) {
        Show-Usage
        return
    }

    # Ensure cleanup happens
    try {
        Write-Info "Starting SNP installation..."

        # Get version to install
        if (-not $Version) {
            $Version = Get-LatestVersion
            Write-Info "Latest version: $Version"
        }
        else {
            Write-Info "Installing version: $Version"
        }

        # Download and install
        $BinaryPath = Download-Binary -Version $Version
        Install-Binary -BinaryPath $BinaryPath

        Write-Success "SNP installation completed!"
        Write-Info "Run 'snp --help' to get started"
    }
    finally {
        Remove-TempDirectory
    }
}

# Run main function
Main
