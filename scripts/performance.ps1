# SPDX-FileCopyrightText: 2026 OpenCabinet contributors
# SPDX-License-Identifier: GPL-3.0-or-later

<#
.SYNOPSIS
Measures OpenCabinet Designer against the budgets of ADR-0014 and, when a
base build is given, against that base.

.DESCRIPTION
Runs the head build, and the base build when given, in turns, so both see
the same machine load. A measure fails when it uses more than half of its
budget, or when it is more than 30% worse than the base by more than a small
absolute margin that keeps run-to-run noise from failing a change.
Memory is gated on Windows only: on Linux the virtual display inflates it.
Prints a Markdown table, also appended to the GitHub job summary, and exits
with 1 when a measure fails, unless -Accepted is given: a maintainer has
accepted the cost with the performance-accepted label, and the table records
it instead.
#>
param(
    [Parameter(Mandatory)] [string] $HeadApp,
    [Parameter(Mandatory)] [string] $HeadEdit,
    [string] $BaseApp,
    [string] $BaseEdit,
    [int] $Runs = 7,
    [switch] $Accepted
)

$ErrorActionPreference = 'Stop'
$Marker = 'OPENCABINET_FIRST_FRAME'

# Budget on the reference computer, and the margin below which a change
# against the base is treated as noise.
$Measures = @(
    @{ Key = 'start'; Name = 'Start to first frame'; Unit = 'ms'; Budget = 2000; Noise = 50; Gated = $true; Digits = 1 }
    @{ Key = 'memory'; Name = 'Memory after start'; Unit = 'MB'; Budget = 150; Noise = 5; Gated = $IsWindows; Digits = 1 }
    @{ Key = 'edit'; Name = 'Response to an edit'; Unit = 'ms'; Budget = 50; Noise = 1; Gated = $true; Digits = 3 }
)

function Measure-Start([string] $Exe) {
    $info = [System.Diagnostics.ProcessStartInfo]::new((Resolve-Path $Exe).Path)
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.Environment['OPENCABINET_PERF'] = '1'
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    $process = [System.Diagnostics.Process]::Start($info)
    try {
        while ($true) {
            $read = $process.StandardOutput.ReadLineAsync()
            if (-not $read.Wait(30000) -or $null -eq $read.Result) { return $null }
            if ($read.Result -eq $Marker) { break }
        }
        $milliseconds = $clock.Elapsed.TotalMilliseconds
        # Let the window settle before sampling its memory.
        Start-Sleep -Milliseconds 1000
        $process.Refresh()
        @{ start = $milliseconds; memory = $process.WorkingSet64 / 1MB }
    }
    finally {
        if (-not $process.WaitForExit(10000)) { $process.Kill() }
    }
}

function Measure-Edit([string] $Exe) {
    $output = & $Exe
    if ($LASTEXITCODE -ne 0) { throw "$Exe failed" }
    [double] $output / 1e6
}

function Median([double[]] $Values) {
    $sorted = $Values | Sort-Object
    $sorted[[math]::Floor($sorted.Count / 2)]
}

$builds = [ordered]@{ head = @{ App = $HeadApp; Edit = $HeadEdit } }
if ($BaseApp -and (Test-Path $BaseApp)) {
    $builds.base = @{ App = $BaseApp; Edit = $BaseEdit }
}
$samples = @{}
foreach ($name in $builds.Keys) {
    $samples[$name] = @{ start = @(); memory = @(); edit = @() }
}

# One untimed start each, so the first measured run is not a cold start
# from disk.
foreach ($build in $builds.Values) { $null = Measure-Start $build.App }
for ($run = 0; $run -lt $Runs; $run++) {
    foreach ($name in $builds.Keys) {
        $result = Measure-Start $builds[$name].App
        if ($null -eq $result) {
            if ($name -eq 'head') { throw 'the head build never showed its first frame' }
            # A base from before this harness cannot be measured.
            $builds.Remove('base')
            break
        }
        $samples[$name].start += $result.start
        $samples[$name].memory += $result.memory
    }
}
foreach ($name in @($builds.Keys)) {
    $edit = $builds[$name].Edit
    if ($edit -and (Test-Path $edit)) {
        $samples[$name].edit = 1..5 | ForEach-Object { Measure-Edit $edit }
    }
}

$failed = $false
$lines = @(
    "| Measure | Budget | Head | Base | Result |"
    "| --- | --- | --- | --- | --- |"
)
foreach ($measure in $Measures) {
    $key = $measure.Key
    $unit = $measure.Unit
    $head = Median $samples.head[$key]
    $base = if ($builds.Contains('base') -and $samples.base[$key].Count) { Median $samples.base[$key] }
    $problems = @()
    if ($head -gt $measure.Budget / 2) { $problems += 'over half the budget' }
    if ($null -ne $base -and $head -gt $base * 1.3 -and $head - $base -gt $measure.Noise) {
        $problems += 'more than 30% worse than the base'
    }
    $result = if (-not $problems) { 'ok' }
              elseif ($measure.Gated -and $Accepted) { 'ACCEPTED: ' + ($problems -join ', ') }
              elseif ($measure.Gated) { $failed = $true; 'FAIL: ' + ($problems -join ', ') }
              else { 'not gated: ' + ($problems -join ', ') }
    $number = "N$($measure.Digits)"
    $baseText = if ($null -ne $base) { "{0:$number} $unit" -f $base } else { 'n/a' }
    $lines += "| {0} | {1} $unit | {2:$number} $unit | {3} | {4} |" -f `
        $measure.Name, $measure.Budget, $head, $baseText, $result
}
$lines += ''
$lines += "Medians of $Runs runs ($(if ($IsWindows) { 'Windows' } else { 'Linux' }))."
if ($Accepted) {
    $lines += 'A maintainer accepted this cost with the `performance-accepted` label.'
}

$lines | Write-Output
if ($env:GITHUB_STEP_SUMMARY) { $lines | Out-File -Append -Encoding utf8 $env:GITHUB_STEP_SUMMARY }
if ($failed) { exit 1 }
