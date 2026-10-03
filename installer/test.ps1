# Test install.ps1 end to end, the way `irm ... | iex` runs it, under the
# PowerShell running this file; CI runs it under both Windows PowerShell 5.1
# and PowerShell 7. The rendered script in $Dir\release must install the
# executable it names. Pointed at $Dir\tampered, where that executable has
# other bytes, it must refuse, leave nothing installed, and not end the
# session that ran it. Neither run may leave variables or preferences in
# that session.
#
# Usage: installer/test.ps1 -Dir DIR -Template installer/install.ps1 [-Asset windows-x64]
#   DIR       holds release\ (from installer/render.sh, with executables
#             that answer `version`) and tampered\
#   Template  the unrendered install.ps1, which must refuse to run
#   Asset     the build this machine should get; when given, the installed
#             executable's `version` output must name it
# Needs python on PATH for the HTTP server.
param(
  [Parameter(Mandatory)][string]$Dir,
  [Parameter(Mandatory)][string]$Template,
  [string]$Asset
)
$ErrorActionPreference = 'Stop'
# The server runs in another process, which does not share this location.
$Dir = (Resolve-Path -LiteralPath $Dir).Path
$Template = (Resolve-Path -LiteralPath $Template).Path
$failures = 0
function Fail([string]$message) { Write-Host "FAIL: $message"; $script:failures++ }
function Pass([string]$message) { Write-Host "ok: $message" }

$edition = "PowerShell $($PSVersionTable.PSVersion)"
$work = Join-Path ([IO.Path]::GetTempPath()) ('packslip-test-' + [IO.Path]::GetRandomFileName())
$null = New-Item -ItemType Directory -Path $work

# Serve $Dir on a free port, with .ps1 as text the way packslip.sh serves it.
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
$listener.Stop()
$serverScript = Join-Path $work 'serve.py'
Set-Content -LiteralPath $serverScript -Encoding ascii -Value @'
import functools, http.server, sys
class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".ps1": "text/plain; charset=utf-8"}
    def log_message(self, *args):
        pass
handler = functools.partial(Handler, directory=sys.argv[2])
http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), handler).serve_forever()
'@
$server = Start-Process python -ArgumentList "`"$serverScript`"", $port, "`"$Dir`"" -PassThru -WindowStyle Hidden
$base = "http://127.0.0.1:$port"

try {
  foreach ($attempt in 1..50) {
    try { $null = Invoke-WebRequest -UseBasicParsing "$base/release/install.ps1"; break } catch { Start-Sleep -Milliseconds 200 }
  }

  # Run the script as `irm URL | iex` does, into a directory of its own.
  function Install-From([string]$name, [string]$releaseUrl) {
    $binDir = Join-Path $work $name
    $env:PACKSLIP_RELEASE_URL = $releaseUrl
    $env:PACKSLIP_BIN_DIR = $binDir
    try {
      $output = & { Invoke-RestMethod "$base/release/install.ps1" | Invoke-Expression } *>&1 | Out-String
      [pscustomobject]@{ BinDir = $binDir; Error = $null; Output = $output }
    } catch {
      [pscustomobject]@{ BinDir = $binDir; Error = $_; Output = '' }
    } finally {
      Remove-Item Env:PACKSLIP_RELEASE_URL, Env:PACKSLIP_BIN_DIR -ErrorAction SilentlyContinue
    }
  }
  function Leftovers([string]$binDir) {
    @(Get-ChildItem -LiteralPath $binDir -Force -Filter '.packslip-*' -ErrorAction SilentlyContinue)
  }

  # The first run as at a prompt: `irm | iex` in this scope, so anything
  # the script failed to keep to itself would land here.
  $progress = $ProgressPreference
  $result = [pscustomobject]@{ BinDir = (Join-Path $work 'fresh'); Error = $null; Output = '' }
  $env:PACKSLIP_RELEASE_URL = "$base/release"
  $env:PACKSLIP_BIN_DIR = $result.BinDir
  $before = @(Get-Variable | ForEach-Object { $_.Name })
  try {
    $result.Output = Invoke-RestMethod "$base/release/install.ps1" | Invoke-Expression *>&1 | Out-String
  } catch {
    $result.Error = $_
  } finally {
    Remove-Item Env:PACKSLIP_RELEASE_URL, Env:PACKSLIP_BIN_DIR -ErrorAction SilentlyContinue
  }
  # LASTEXITCODE is PowerShell's own, set by any native command.
  $leaked = @(Get-Variable | ForEach-Object { $_.Name } | Where-Object { $_ -notin $before -and $_ -notin 'before', 'LASTEXITCODE' })
  $exe = Join-Path $result.BinDir 'packslip.exe'
  if ($result.Error) {
    Fail "${edition}: install failed: $($result.Error)"
  } elseif (-not (Test-Path -LiteralPath $exe)) {
    Fail "${edition}: no packslip.exe after install: $($result.Output)"
  } else {
    $said = & $exe version
    if ($LASTEXITCODE -ne 0) {
      Fail "${edition}: packslip.exe version exited $LASTEXITCODE"
    } elseif ($Asset -and "$said" -notmatch [regex]::Escape("($Asset)")) {
      Fail "${edition}: installed the wrong build, which says: $said"
    } else {
      Pass "${edition}: installs and runs packslip.exe $(if ($Asset) { "for $Asset" })"
    }
  }
  if ($result.Output -match 'is not on PATH') { Pass "${edition}: says the directory is not on PATH" } else { Fail "${edition}: no PATH hint: $($result.Output)" }
  if ((Leftovers $result.BinDir).Count -eq 0) { Pass "${edition}: leaves no temporary file" } else { Fail "${edition}: left a temporary file" }

  # Nothing leaks into the session that ran it.
  if ($leaked) { Fail "${edition}: leaked variables: $leaked" } else { Pass "${edition}: leaks no variables" }
  if ($ProgressPreference -eq $progress) { Pass "${edition}: leaves the session's preferences alone" } else { Fail "${edition}: changed `$ProgressPreference" }

  # Over the copy just installed, and with the directory on PATH.
  $env:Path = "$($result.BinDir);$env:Path"
  $again = Install-From 'fresh' "$base/release"
  if ($again.Error) { Fail "${edition}: reinstall failed: $($again.Error)" } else { Pass "${edition}: reinstalls" }
  if ($again.Output -notmatch 'is not on PATH') { Pass "${edition}: no PATH hint when the directory is on PATH" } else { Fail "${edition}: PATH hint although on PATH" }

  $tampered = Install-From 'tampered' "$base/tampered"
  if (-not $tampered.Error) {
    Fail "${edition}: installed a tampered download"
  } elseif ("$($tampered.Error)" -match 'refusing') {
    Pass "${edition}: refuses a tampered download"
  } else {
    Fail "${edition}: tampered download failed the wrong way: $($tampered.Error)"
  }
  if (Test-Path -LiteralPath (Join-Path $tampered.BinDir 'packslip.exe')) { Fail "${edition}: a tampered download was left installed" }
  if ((Leftovers $tampered.BinDir).Count -ne 0) { Fail "${edition}: a tampered download left a temporary file" }
  Pass "${edition}: the session survives a refused install"

  $templateError = $null
  try { Get-Content -Raw -LiteralPath $Template | Invoke-Expression } catch { $templateError = $_ }
  if ("$templateError" -match 'this is the template') { Pass "${edition}: refuses to run the unrendered template" } else { Fail "${edition}: the template ran or failed the wrong way: $templateError" }
} finally {
  Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
  Remove-Item -Recurse -Force -LiteralPath $work -ErrorAction SilentlyContinue
}

if ($failures -gt 0) {
  Write-Host "$failures check(s) failed"
  exit 1
}
Write-Host "all install.ps1 checks passed under $edition"
