param([string]$AppPath, [string]$ModelsDir, [string]$Screenshot, [switch]$FastOnly,
      [string]$OutputDir = (Join-Path ([Environment]::GetFolderPath('Desktop')) 'OCRTrans-release-check'),
      [string]$ExpectedVersion = '0.6.7')
$ErrorActionPreference = 'Stop'
if (!$AppPath) {
    $running = Get-Process -Name ocr-trans -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($running) { $AppPath = $running.Path }
}
if (!$AppPath) {
    Add-Type -AssemblyName System.Windows.Forms
    $picker = New-Object Windows.Forms.OpenFileDialog
    $picker.Filter = 'OCR Trans|ocr-trans.exe'
    if ($picker.ShowDialog() -ne 'OK') { throw 'Choose the installed ocr-trans.exe to test.' }
    $AppPath = $picker.FileName
}
$AppPath = (Resolve-Path -LiteralPath $AppPath).Path
$version = [Diagnostics.FileVersionInfo]::GetVersionInfo($AppPath).ProductVersion
$versionPattern = '^' + [regex]::Escape($ExpectedVersion) + '(?:\.0)?$'
if ($version -notmatch $versionPattern) { throw "Expected $ExpectedVersion, found $version at $AppPath" }
if (!$Screenshot) {
    $Screenshot = Join-Path $PSScriptRoot 'en_small.png'
    if (!(Test-Path -LiteralPath $Screenshot)) { $Screenshot = Join-Path $PSScriptRoot '../testdata/en_small.png' }
}
$Screenshot = (Resolve-Path -LiteralPath $Screenshot).Path
if (!$ModelsDir -and $FastOnly) { $ModelsDir = Join-Path (Split-Path $AppPath) 'models' }
if (!$ModelsDir) {
    $data = Join-Path $env:APPDATA 'com.sinkfish.ocrtrans'
    $config = Join-Path $data 'settings.json'
    $candidate = if (Test-Path -LiteralPath $config) {
        (Get-Content -LiteralPath $config -Raw -Encoding UTF8 | ConvertFrom-Json).models_dir
    }
    $ModelsDir = @($candidate, (Join-Path $data 'models'), (Join-Path (Split-Path $AppPath) 'models')) |
        Where-Object { $_ -and (Test-Path -LiteralPath (Join-Path $_ 'qwen3-1.7b/model.gguf')) } | Select-Object -First 1
}
if (!$ModelsDir) { throw 'Install Qwen in Settings or provide -ModelsDir containing qwen3-1.7b/model.gguf.' }
$ModelsDir = (Resolve-Path -LiteralPath $ModelsDir).Path
[IO.Directory]::CreateDirectory($OutputDir) | Out-Null
$summary = @("OCR Trans $version", ("Executable SHA256 " + (Get-FileHash -LiteralPath $AppPath -Algorithm SHA256).Hash),
             ("Windows " + [Environment]::OSVersion.VersionString))
$cases = if ($FastOnly) { @('fast') } else { @('fast', 'quality-cpu', 'quality-auto') }
foreach ($case in $cases) {
    $profileDir = Join-Path ([IO.Path]::GetTempPath()) ('ocrtrans-test-' + [guid]::NewGuid())
    [IO.Directory]::CreateDirectory($profileDir) | Out-Null
    $engine = if ($case -eq 'fast') { 'fast' } else { 'quality' }
    $expected = if ($case -eq 'fast') { 'nmt' } else { 'llm' }
    $device = if ($case -eq 'quality-cpu') { 'cpu' } else { 'auto' }
    $settings = @{version=2; onboarded=$true; hotkey_capture=''; keep_history=$false;
                  engine=$engine; llm_device=$device; models_dir=$ModelsDir}
    $utf8 = New-Object Text.UTF8Encoding($false)
    [IO.File]::WriteAllText((Join-Path $profileDir 'settings.json'), ($settings | ConvertTo-Json), $utf8)
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $AppPath
    $start.WorkingDirectory = $profileDir
    $start.UseShellExecute = $false
    $start.EnvironmentVariables['OCRTRANS_TEST_PROFILE'] = $profileDir
    $start.EnvironmentVariables['OCRTRANS_DATA_DIR'] = $profileDir
    $start.EnvironmentVariables['OCRTRANS_TEST_PIN'] = $Screenshot + ':translate'
    $start.EnvironmentVariables['OCRTRANS_ENGINE'] = $engine
    $process = $null
    try {
        $process = [Diagnostics.Process]::Start($start)
        if (!$process.WaitForExit(300000)) { throw "$case timed out" }
        $log = [IO.File]::ReadAllText((Join-Path $profileDir 'ocr-trans.log'))
        if ($process.ExitCode -ne 0 -or !$log.Contains("release-check displayed engine=$expected warnings=0")) {
            throw "$case did not complete normal OCR and translation rendering"
        }
        if ($device -eq 'cpu' -and !$log.Contains('llm: model on CPU')) { throw 'CPU selection failed' }
        $actualDevice = if ($log -match 'quality worker: llm: model on ([^\r\n]+)') { $Matches[1] } else { 'fast NMT' }
        $summary += "$case PASS ($actualDevice)"
    } catch {
        $summary += "$case FAIL: $($_.Exception.Message)"
        throw
    } finally {
        if ($process -and !$process.HasExited) { $process.Kill(); $process.WaitForExit() }
        if ($process) { $process.Dispose() }
        $logPath = Join-Path $profileDir 'ocr-trans.log'
        if (Test-Path $logPath) { Copy-Item -LiteralPath $logPath -Destination (Join-Path $OutputDir "$case.log") -Force }
        [IO.File]::WriteAllLines((Join-Path $OutputDir 'result.txt'), [string[]]$summary, $utf8)
        Remove-Item -LiteralPath $profileDir -Recurse -Force
    }
}
$summary
Write-Host "Report: $OutputDir"
