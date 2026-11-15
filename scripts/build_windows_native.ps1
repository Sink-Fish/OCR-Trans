#requires -Version 7.3
# Run in an x64 MSVC developer shell with Rust, Vulkan SDK, LLVM, pnpm, Python and patch.
param([Parameter(Mandatory)][string]$Assets)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location $root
foreach ($name in @('rustc', 'cargo', 'pnpm', 'cl', 'cmake', 'llvm-readobj', 'python', 'patch', '7z')) {
    Get-Command $name -ErrorAction Stop | Out-Null
}
if (!$env:VULKAN_SDK) { throw 'A Vulkan SDK installation is required.' }
foreach ($name in @('models','runtime')) {
    $source = (Resolve-Path (Join-Path $Assets $name)).Path
    $destination = Join-Path $root $name
    if ($source -ne $destination) {
        New-Item -ItemType Directory -Force $destination | Out-Null
        Copy-Item -Path (Join-Path $source '*') -Destination $destination -Recurse -Force
    }
}
if ((Get-FileHash runtime/onnxruntime.dll -Algorithm SHA256).Hash -ne '0c42dc4e278f0968fdf9ebf1b1e54a74c657a9942e70803e453c91993451cf4c') {
    throw 'ONNX Runtime failed release checksum verification.'
}
$target = 'x86_64-pc-windows-msvc'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
$env:LLAMA_STATIC_CRT = '0'
$stage = Join-Path $root 'target/native-release'
New-Item -ItemType Directory -Force $stage | Out-Null
$originalLock = [IO.File]::ReadAllBytes((Join-Path $root 'Cargo.lock'))
try {
    pnpm install --frozen-lockfile
    pnpm build
    $metadata = (cargo metadata --locked --format-version 1 | Out-String) | ConvertFrom-Json
    $llama = $metadata.packages | Where-Object { $_.name -eq 'llama-cpp-sys-2' -and $_.version -eq '0.1.160' }
    if (!$llama) { throw 'Pinned llama-cpp-sys-2 was not resolved.' }
    $patchFile = Join-Path $PSScriptRoot 'patches/llama-cpp-sys-2-clang-cl.patch'
    $vendor = Join-Path $stage ('llama-' + (Get-FileHash $patchFile).Hash.Substring(0,12))
    if (!(Test-Path (Join-Path $vendor '.release-patched'))) {
        if (Test-Path $vendor) { Remove-Item -Recurse -Force -LiteralPath $vendor }
        Copy-Item -LiteralPath (Split-Path $llama.manifest_path) -Destination $vendor -Recurse
        patch -d $vendor -p1 -i $patchFile
        New-Item -ItemType File (Join-Path $vendor '.release-patched') | Out-Null
    }
    $patch = 'patch.crates-io.llama-cpp-sys-2.path="' + $vendor.Replace('\','/') + '"'
    python scripts/dev/windows_crt.py prepare "target/$target/release/build"
    $messages = cargo build --release --target $target -p ocr-trans --config $patch --message-format=json-render-diagnostics
    $output = $messages | ForEach-Object { $_ | ConvertFrom-Json } |
        Where-Object { $_.reason -eq 'build-script-executed' -and $_.package_id -match 'llama-cpp-sys-2' } |
        Select-Object -Last 1 -ExpandProperty out_dir
    if (!$output) { throw 'Native model build output missing.' }
    python scripts/dev/windows_crt.py verify $output
    $dlls = @{}
    foreach ($dir in @('bin','backends')) {
        Get-ChildItem (Join-Path $output $dir) -Filter '*.dll' | ForEach-Object { $dlls[$_.FullName] = $_.Name }
    }
    $config = Join-Path $stage 'resources.json'
    @{bundle=@{resources=$dlls}} | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8NoBOM $config
    pnpm tauri build --target $target --bundles nsis --config $config -- --config $patch
    $version = (Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version
    New-Item -ItemType Directory -Force release | Out-Null
    $setup = "target/$target/release/bundle/nsis/OCR Trans_${version}_x64-setup.exe"
    python scripts/dev/verify_windows_installer.py $setup $output "target/$target/release/ocr-trans.exe" $version --sevenzip 7z |
        Set-Content -Encoding utf8NoBOM release/native-payload-check.json
    Copy-Item -LiteralPath $setup -Destination "release/OCR-Trans_${version}_native_x64-setup.exe" -Force
} finally {
    [IO.File]::WriteAllBytes((Join-Path $root 'Cargo.lock'), $originalLock)
}
