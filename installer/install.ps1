# Install packslip @PACKSLIP_VERSION@ on Windows, from PowerShell:
#
#   irm https://packslip.sh/install.ps1 | iex
#
# This script downloads the packslip executable built for this machine and
# refuses it unless its SHA-256 is the one written below when the release
# was built. Then it puts the executable in a directory for PATH. It
# downloads and runs nothing else. Each release publishes its own copy,
# attested by packslip's release workflow:
#
#   gh attestation verify install.ps1 --repo jdx/packslip
#
# https://packslip.sh/v@PACKSLIP_VERSION@/install.ps1 always serves this file.
#
# Environment:
#   PACKSLIP_BIN_DIR      the directory to install into: ~\.local\bin when unset
#   PACKSLIP_RELEASE_URL  where the release files are, such as a mirror,
#                         instead of https://packslip.dev/v@PACKSLIP_VERSION@;
#                         the checksums below hold whatever serves them
#
# The script block keeps its variables and preferences out of the session
# that runs it, and it reports failure by throwing: `exit` under
# Invoke-Expression would close that session's window.
& {
  $ErrorActionPreference = 'Stop'
  # Windows PowerShell's progress bar slows a download down severalfold.
  $ProgressPreference = 'SilentlyContinue'

  $version = '@PACKSLIP_VERSION@'
  $checksums = @{
    'windows-x64'   = '@SHA256_WINDOWS_X64@'
    'windows-arm64' = '@SHA256_WINDOWS_ARM64@'
  }

  if ($version -notmatch '^[0-9]') {
    throw "packslip install: this is the template in packslip's repository; run the copy a release publishes, from https://packslip.sh/install.ps1"
  }
  if ($PSVersionTable.PSEdition -eq 'Core' -and -not $IsWindows) {
    throw 'packslip install: this script is for Windows; on Linux and macOS run: curl -fsSL https://packslip.sh | sh'
  }

  # The OS architecture rather than this process's, so an x64 PowerShell
  # emulated on an ARM64 machine still gets the ARM64 build. Older .NET
  # lacks RuntimeInformation; the environment says the same there.
  $arch = $null
  try {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
  } catch {
    $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
  }
  # Hashtable keys match without regard to case: X64 and Arm64 from .NET,
  # AMD64 and ARM64 from the environment.
  $asset = @{ 'X64' = 'windows-x64'; 'AMD64' = 'windows-x64'; 'ARM64' = 'windows-arm64' }[[string]$arch]
  if (-not $asset) {
    throw "packslip install: packslip publishes no build for Windows on $arch"
  }
  $want = $checksums[$asset]

  $base = if ($env:PACKSLIP_RELEASE_URL) { $env:PACKSLIP_RELEASE_URL } else { "https://packslip.dev/v$version" }
  $url = $base.TrimEnd('/') + "/packslip-v$version-$asset.exe"
  $binDir = if ($env:PACKSLIP_BIN_DIR) { $env:PACKSLIP_BIN_DIR } else { Join-Path $HOME '.local\bin' }
  $null = New-Item -ItemType Directory -Force -Path $binDir
  $target = Join-Path $binDir 'packslip.exe'

  # Windows PowerShell 5.1 can still default to TLS 1.0.
  if ($PSVersionTable.PSVersion.Major -lt 6) {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
  }

  # Download beside the destination, so the final step is a rename on one
  # volume and an interrupted install leaves no partial executable.
  $tmp = Join-Path $binDir ('.packslip-' + [IO.Path]::GetRandomFileName())
  try {
    Write-Host "packslip install: downloading $url"
    Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $tmp
    $got = (Get-FileHash -Algorithm SHA256 -LiteralPath $tmp).Hash.ToLowerInvariant()
    if ($got -ne $want) {
      throw "packslip install: refusing ${url}: its SHA-256 is $got, but packslip $version's $asset build is $want"
    }
    Move-Item -Force -LiteralPath $tmp -Destination $target
  } finally {
    Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath $tmp
  }

  $installed = & $target version
  if ($LASTEXITCODE -ne 0) {
    throw "packslip install: installed $target, but it does not run on this machine"
  }
  Write-Host "packslip install: installed $installed at $target"

  $onPath = ($env:Path -split ';') | Where-Object { $_ -and $_.TrimEnd('\') -eq $binDir.TrimEnd('\') }
  if (-not $onPath) {
    Write-Host "packslip install: $binDir is not on PATH. To run packslip by name in this session:"
    Write-Host "packslip install:   `$env:Path = `"$binDir;`$env:Path`""
    Write-Host "packslip install: To keep it, add $binDir to your user Path environment variable."
  }
}
