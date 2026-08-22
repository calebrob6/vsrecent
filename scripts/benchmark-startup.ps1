[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$BaselineExecutable,

    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$CandidateExecutable,

    [ValidateRange(1, 1000)]
    [int]$Runs = 20,

    [ValidateRange(0, 100)]
    [int]$WarmupRuns = 3,

    [ValidateRange(1, 30)]
    [int]$TimeoutSeconds = 5
)

$ErrorActionPreference = "Stop"

function Get-PeMachine {
    param([Parameter(Mandatory)][string]$Path)

    $stream = [IO.File]::OpenRead($Path)
    $reader = [IO.BinaryReader]::new($stream)
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) {
            throw "$Path is not a Windows executable."
        }
        $stream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550) {
            throw "$Path does not contain a valid PE header."
        }
        switch ($reader.ReadUInt16()) {
            0x8664 { return "x64" }
            0xAA64 { return "ARM64" }
            default { throw "$Path has an unsupported machine type." }
        }
    }
    finally {
        $reader.Dispose()
        $stream.Dispose()
    }
}

function Measure-Launch {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Label
    )

    $process = $null
    try {
        $startInfo = [Diagnostics.ProcessStartInfo]::new()
        $startInfo.FileName = $Path
        $startInfo.Arguments = "--demo --light"
        $startInfo.UseShellExecute = $false
        $stopwatch = [Diagnostics.Stopwatch]::StartNew()
        $process = [Diagnostics.Process]::Start($startInfo)
        $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
        $window = [IntPtr]::Zero
        while ([DateTime]::UtcNow -lt $deadline) {
            if ($process.HasExited) {
                throw "$Label exited before displaying a window. Close any existing VS Recent window and retry."
            }
            $process.Refresh()
            $window = $process.MainWindowHandle
            if ($window -ne [IntPtr]::Zero) {
                break
            }
            [Threading.Thread]::Sleep(1)
        }
        $stopwatch.Stop()
        if ($window -eq [IntPtr]::Zero) {
            throw "$Label did not display a window within $TimeoutSeconds seconds."
        }
        return $stopwatch.Elapsed.TotalMilliseconds
    }
    finally {
        if ($null -ne $process -and -not $process.HasExited) {
            $process.Kill()
            $process.WaitForExit()
        }
        if ($null -ne $process) {
            $process.Dispose()
        }
    }
}

function Get-Percentile {
    param(
        [Parameter(Mandatory)][double[]]$Values,
        [Parameter(Mandatory)][ValidateRange(0, 100)][double]$Percentile
    )

    $sorted = @($Values | Sort-Object)
    $index = [Math]::Ceiling(($Percentile / 100) * $sorted.Count) - 1
    return $sorted[[Math]::Max(0, $index)]
}

$baseline = (Resolve-Path -LiteralPath $BaselineExecutable).Path
$candidate = (Resolve-Path -LiteralPath $CandidateExecutable).Path
$baselineMachine = Get-PeMachine -Path $baseline
$candidateMachine = Get-PeMachine -Path $candidate
if ($baselineMachine -ne $candidateMachine) {
    throw "Architecture mismatch: baseline is $baselineMachine and candidate is $candidateMachine."
}

if ($WarmupRuns -gt 0) {
    for ($run = 1; $run -le $WarmupRuns; $run++) {
        [void](Measure-Launch -Path $baseline -Label "Baseline warmup")
        [void](Measure-Launch -Path $candidate -Label "Candidate warmup")
    }
}

$baselineTimes = [Collections.Generic.List[double]]::new()
$candidateTimes = [Collections.Generic.List[double]]::new()
for ($run = 1; $run -le $Runs; $run++) {
    Write-Progress -Activity "Benchmarking startup" -Status "Pair $run of $Runs" `
        -PercentComplete (($run / $Runs) * 100)
    if ($run % 2 -eq 1) {
        $baselineTimes.Add((Measure-Launch -Path $baseline -Label "Baseline"))
        $candidateTimes.Add((Measure-Launch -Path $candidate -Label "Candidate"))
    }
    else {
        $candidateTimes.Add((Measure-Launch -Path $candidate -Label "Candidate"))
        $baselineTimes.Add((Measure-Launch -Path $baseline -Label "Baseline"))
    }
}
Write-Progress -Activity "Benchmarking startup" -Completed

$baselineMedian = Get-Percentile -Values $baselineTimes -Percentile 50
$candidateMedian = Get-Percentile -Values $candidateTimes -Percentile 50
$baselineP95 = Get-Percentile -Values $baselineTimes -Percentile 95
$candidateP95 = Get-Percentile -Values $candidateTimes -Percentile 95
$change = (($candidateMedian - $baselineMedian) / $baselineMedian) * 100

@(
    [pscustomobject]@{
        Build = "Baseline"
        Architecture = $baselineMachine
        Runs = $Runs
        MedianMs = [Math]::Round($baselineMedian, 2)
        P95Ms = [Math]::Round($baselineP95, 2)
    }
    [pscustomobject]@{
        Build = "Candidate"
        Architecture = $candidateMachine
        Runs = $Runs
        MedianMs = [Math]::Round($candidateMedian, 2)
        P95Ms = [Math]::Round($candidateP95, 2)
    }
) | Format-Table -AutoSize

Write-Host ("Median change: {0:+0.00;-0.00;0.00}%" -f $change)
