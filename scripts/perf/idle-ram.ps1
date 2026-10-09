# NFR-004: the memory of dzbuild-pos.exe and of every process it started (the WebView2 browser,
# GPU, utility and renderer processes), in MB, per process and in total, both as the working set
# (which counts the pages WebView2's processes share once per process) and as the private working
# set (Win32_PerfFormattedData_PerfProc_Process.WorkingSetPrivate, the pages only that process
# holds). Both are printed; NFR-004 is judged on the private working set. Measure with no WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS (a DevTools port adds its own).
# Usage: .\idle-ram.ps1 [-WaitSeconds 60]
param([int]$WaitSeconds = 60)
if ($env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS) {
  Write-Warning "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS is set ($env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS): the figure is not the shop's"
}
Start-Sleep -Seconds $WaitSeconds
$root = Get-CimInstance Win32_Process -Filter "Name = 'dzbuild-pos.exe'" | Select-Object -First 1
if (-not $root) { Write-Error "DZBuild POS is not running"; exit 1 }
$all = Get-CimInstance Win32_Process
$ids = [System.Collections.Generic.HashSet[uint32]]::new()
[void]$ids.Add($root.ProcessId)
do {
  $added = 0
  foreach ($p in $all) {
    if ($ids.Contains($p.ParentProcessId) -and $ids.Add($p.ProcessId)) { $added++ }
  }
} while ($added -gt 0)
$private = @{}
foreach ($perf in Get-CimInstance Win32_PerfFormattedData_PerfProc_Process) {
  $private[[uint32]$perf.IDProcess] = [uint64]$perf.WorkingSetPrivate
}
$wsTotal = [uint64]0
$privateTotal = [uint64]0
foreach ($p in ($all | Where-Object { $ids.Contains($_.ProcessId) } | Sort-Object ProcessId)) {
  $ws = [uint64]$p.WorkingSetSize
  $own = if ($private.ContainsKey([uint32]$p.ProcessId)) { $private[[uint32]$p.ProcessId] } else { [uint64]0 }
  $wsTotal += $ws
  $privateTotal += $own
  Write-Output ("IDLE_RAM_PROCESS {0} {1} ws {2} MB private {3} MB" -f $p.ProcessId, $p.Name, [math]::Round($ws / 1MB), [math]::Round($own / 1MB))
}
$wsMb = [math]::Round($wsTotal / 1MB)
$privateMb = [math]::Round($privateTotal / 1MB)
$verdict = { param($mb) if ($mb -le 250) { 'OK' } else { 'OVER' } }
Write-Output ("IDLE_RAM_MB ws {0} ({1}) private {2} ({3}) budget 250" -f $wsMb, (& $verdict $wsMb), $privateMb, (& $verdict $privateMb))
