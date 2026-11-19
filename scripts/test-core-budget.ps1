#Requires -Version 7.2
[CmdletBinding()]
param(
    [string] $TestFilter,
    [switch] $IncludeIgnored
)

# Local Windows QA. Keep compiler writes on the host, outside the ReFS VHDX.
$ErrorActionPreference = 'Stop'
$manifest = Join-Path $PSScriptRoot '../packages/duel-core/Cargo.toml' -Resolve
$target = Join-Path $env:LOCALAPPDATA 'my-awesome-kill/p1-cargo-target'
$drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($target))
if ($drive.Name -ne 'C:\') { throw "Expected a cache on C:, got $target" }
for ($dir = [IO.DirectoryInfo]::new($target); $dir; $dir = $dir.Parent) {
    if ($dir.Exists -and ($dir.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Cache path crosses a reparse point: $dir"
    }
}
$workspaceImage = 'C:\Work\Work.vhdx'
function Get-ImageBytes {
    if (Test-Path -LiteralPath $workspaceImage) { (Get-Item -LiteralPath $workspaceImage).Length } else { 0 }
}
function Get-CacheBytes {
    $sum = 0L
    if ([IO.Directory]::Exists($target)) {
        foreach ($file in (Get-ChildItem -LiteralPath $target -Recurse -File -Force)) { $sum += $file.Length }
    }
    $sum
}
$initialFree = $drive.AvailableFreeSpace
$initialImage = Get-ImageBytes
$cacheLimit = 2GB
function Measure-Budget {
    $cache = Get-CacheBytes
    $free = $drive.AvailableFreeSpace
    $imageGrowth = (Get-ImageBytes) - $initialImage
    if ($cache -ge $cacheLimit -or $free -lt 120GB -or $initialFree - $free -ge 3GB -or $imageGrowth -ge 256MB) {
        throw "Storage limit reached: cache=$cache, free on C:=$free, VHDX growth=$imageGrowth (bytes)"
    }
    $cache
}
$initialCache = Measure-Budget
$start = [Diagnostics.ProcessStartInfo]::new((Get-Command cargo.exe -CommandType Application).Source)
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$start.WorkingDirectory = Split-Path $manifest
$start.Environment['CARGO_TARGET_DIR'] = $target
$start.Environment['CARGO_INCREMENTAL'] = '0'
$start.Environment['CARGO_BUILD_JOBS'] = '2'
foreach ($arg in @('test','--locked','--offline','--release','--manifest-path',$manifest)) { $start.ArgumentList.Add($arg) }
if ($TestFilter) { $start.ArgumentList.Add($TestFilter) }
if ($IncludeIgnored) {
    if (!$TestFilter) { throw 'Select the specific long test with -TestFilter.' }
    foreach ($arg in @('--','--ignored','--nocapture')) { $start.ArgumentList.Add($arg) }
}
$peak = $initialCache
$process = [Diagnostics.Process]::Start($start)
try {
    $outText = $process.StandardOutput.ReadToEndAsync()
    $errText = $process.StandardError.ReadToEndAsync()
    while (!$process.WaitForExit(1000)) { $peak = [math]::Max($peak, (Measure-Budget)) }
    [Console]::Out.Write($outText.GetAwaiter().GetResult())
    [Console]::Error.Write($errText.GetAwaiter().GetResult())
    $final = Measure-Budget
    $peak = [math]::Max($peak, $final)
    Write-Host ("Cargo cache MiB: start={0:N1} sampled peak={1:N1} end={2:N1}; C: free={3:N2} GiB; VHDX growth={4:N1} MiB" -f ($initialCache/1MB),($peak/1MB),($final/1MB),($drive.AvailableFreeSpace/1GB),(((Get-ImageBytes)-$initialImage)/1MB))
    exit $process.ExitCode
} finally {
    if (!$process.HasExited) { $process.Kill($true); $process.WaitForExit() }
    $process.Dispose()
}
