# The reference-PC benchmark (docs/release/reference-pc.md). Writes docs\release\perf-<date>.md.
# Usage, from a checkout of the release's tag with that release installed:
#   powershell -ExecutionPolicy Bypass -File scripts\perf\bench.ps1 [-Exe <path>] [-Runs 5]
param(
  [string]$Exe = (Join-Path $env:ProgramFiles "DZBuild POS\dzbuild-pos.exe"),
  [int]$Runs = 5
)
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$logs = Join-Path $env:APPDATA "dev.dzbuild.pos\logs"
if (-not (Test-Path -LiteralPath $Exe)) { throw "DZBuild POS is not installed at $Exe (pass -Exe)" }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw "cargo is missing (docs/release/reference-pc.md)" }

# Every cold_start_ms the app logged ("first screen ready", written by app_ready), oldest first.
function Get-ColdStarts {
  if (-not (Test-Path -LiteralPath $logs)) { return @() }
  @(Get-ChildItem -LiteralPath $logs -Filter "dzbuild-pos*.log" | Sort-Object LastWriteTime |
      Select-Object -Last 2 | Get-Content | Select-String -Pattern "cold_start_ms=(\d+)" |
      ForEach-Object { [int]$_.Matches[0].Groups[1].Value })
}

# Closes the app as a person would (the backup on close may run). A killed app makes the next
# start an unclean one, which runs the integrity check and slows the very start being measured.
function Stop-Pos {
  foreach ($p in @(Get-Process dzbuild-pos -ErrorAction SilentlyContinue)) {
    for ($try = 0; $try -lt 3 -and -not $p.HasExited; $try++) {
      $p.Refresh()
      [void]$p.CloseMainWindow()
      [void]$p.WaitForExit(60000)
    }
    if (-not $p.HasExited) {
      Write-Warning "DZBuild POS did not close: killed (the next start runs the integrity check)"
      $p.Kill()
      $p.WaitForExit()
    }
  }
}

$lines = [System.Collections.Generic.List[string]]::new()
$cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name
$ram = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)
$os = (Get-CimInstance Win32_OperatingSystem).Caption
$disk = try { (Get-PhysicalDisk | Select-Object -First 1).MediaType } catch { "unknown" }
$lines.Add("# Reference PC benchmark $(Get-Date -Format 'yyyy-MM-dd HH:mm')")
$lines.Add("")
$lines.Add("PC: $cpu, $ram GB, disk $disk, $os. DZBuild POS $((Get-Item -LiteralPath $Exe).VersionInfo.ProductVersion).")
$lines.Add("")

# The release-mode budget tests. cargo writes its progress on stderr, which Windows PowerShell
# turns into errors under "Stop": they run under "Continue" and only the BUDGET lines are kept.
Push-Location $repo
try {
  foreach ($crate in @("pos-core", "pos-hw")) {
    $ErrorActionPreference = "Continue"
    $out = & cargo test --release -p $crate budget_ -- --ignored --nocapture --test-threads=1 2>&1 |
      ForEach-Object { "$_" }
    $code = $LASTEXITCODE
    $ErrorActionPreference = "Stop"
    $budgets = @($out | Where-Object { $_ -match "^BUDGET " })
    foreach ($line in $budgets) { $lines.Add("- $line") }
    if ($budgets.Count -eq 0) { $lines.Add("- $crate`: no BUDGET line (cargo test exited $code)") }
  }
} finally {
  Pop-Location
}

# Cold start: run 1 is the first start (reboot the PC before the benchmark for a cold disk cache).
$starts = [System.Collections.Generic.List[int]]::new()
for ($i = 1; $i -le $Runs; $i++) {
  Stop-Pos
  Start-Sleep -Seconds 5
  $before = @(Get-ColdStarts).Count
  Start-Process -FilePath $Exe
  $deadline = (Get-Date).AddSeconds(60)
  do {
    Start-Sleep -Seconds 2
    $seen = @(Get-ColdStarts)
  } while ($seen.Count -le $before -and (Get-Date) -lt $deadline)
  if ($seen.Count -gt $before) {
    $starts.Add($seen[-1])
    $lines.Add("- cold start run $i`: $($seen[-1]) ms")
  } else {
    $lines.Add("- cold start run $i`: no cold_start_ms line within 60 s")
  }
  # Let the start-up work (checks, the daily backup) finish before the next close.
  Start-Sleep -Seconds 10
}
if ($starts.Count -gt 0) {
  $sorted = @($starts | Sort-Object)
  $median = $sorted[[math]::Floor($sorted.Count / 2)]
  $lines.Add("- BUDGET cold_start $median ms budget 3000 ms $(if ($median -le 3000) { 'OK' } else { 'OVER' })")
} else {
  $lines.Add("- BUDGET cold_start none measured budget 3000 ms OVER")
}

Read-Host "Log in and open the sale screen, leave it alone, then press Enter (the memory is read 60 s later)"
# Per process, then the total: the working set and the private working set (finding 29).
foreach ($idle in (& (Join-Path $PSScriptRoot "idle-ram.ps1") -WaitSeconds 60)) {
  $lines.Add("- $idle")
}

$target = Join-Path $repo "docs\release\perf-$(Get-Date -Format 'yyyyMMdd').md"
$lines | Set-Content -Encoding utf8 -LiteralPath $target
Write-Output "written $target"
