#Requires -Version 5.1
$ErrorActionPreference = 'Stop'

$here = $PSScriptRoot
$id = 'witchy-fi-playa'

function Invoke-Git {
    $ErrorActionPreference = 'Continue'
    $out = & git -C $here @args 2>$null
    if ($LASTEXITCODE -eq 0) { "$out".Trim() }
}

$root = Invoke-Git rev-parse --show-toplevel
if (-not $root) { throw "$here is not inside a git repository" }
$version = $env:VERSION
if (-not $version) {
    $version = (Select-String -Path (Join-Path $here 'Cargo.toml') -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches[0].Groups[1].Value
}

cargo build --release -p $id --manifest-path "$root/Cargo.toml"
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$stage = New-Item -ItemType Directory -Path (Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid()))
try {
    New-Item -ItemType Directory -Path "$stage\bin\windows" | Out-Null
    Copy-Item "$root\target\release\$id.exe" "$stage\bin\windows\$id.exe"
    Copy-Item "$here\assets\icon.png", "$here\assets\rectangle_cover.png", "$here\assets\square_cover.png" $stage
    Copy-Item "$here\content" "$stage\content" -Recurse

    $run = [ordered]@{}
    $executables = @()
    if ($env:LINUX_BIN) {
        New-Item -ItemType Directory -Path "$stage\bin\linux" | Out-Null
        Copy-Item $env:LINUX_BIN "$stage\bin\linux\$id"
        $run.linux = [ordered]@{ command = "bin/linux/$id" }
        $executables = @("bin/linux/$id")
    }
    $run.windows = [ordered]@{ command = "bin/windows/$id.exe" }

    $repo = Invoke-Git remote get-url origin
    if ($repo) { $repo = $repo -replace '\.git$', '' -replace '^git@([^:]*):', 'https://$1/' }
    $tag = Invoke-Git describe --tags --exact-match
    $config = [ordered]@{
        yasaff = 'app'
        formatVersion = 1
        id = $id
        name = 'WitchyFiPlaya'
        version = $version
        kind = 'app'
        author = 'Krystian Grzelak'
        summary = 'A simple music player you control with a gamepad'
        description = "Plays the music files (MP3, FLAC, WAV, OGG, M4A, AIFF and more) in your Music folder (or any folder you pick with the controller), while a little witch dances along (the animations live in the content folder).`n`nA plays a track, X pauses, LB/RB skip, the D-pad seeks, the triggers change the volume, Y shuffles and View repeats. Menu opens the settings (music folder, Xbox or PlayStation button prompts). Xbox and PlayStation controllers both work. Press Guide to go back to YASAFF while the music keeps playing; B quits."
        icon = 'icon.png'
        artwork = [ordered]@{
            cover = 'rectangle_cover.png'
            squareCover = 'square_cover.png'
        }
        run = $run
        install = [ordered]@{ executables = $executables }
        git = [ordered]@{
            repository = if ($repo) { $repo } else { $null }
            branch = Invoke-Git rev-parse --abbrev-ref HEAD
            commit = Invoke-Git rev-parse HEAD
            tag = if ($tag) { $tag } else { $null }
            dirty = [bool](Invoke-Git status --porcelain -- $here)
        }
    }
    [IO.File]::WriteAllText("$stage\yasaffConfig.json", ($config | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))

    $dist = New-Item -ItemType Directory -Path (Join-Path $here 'dist') -Force
    $out = Join-Path $dist "$id-$version.zip"
    Remove-Item $out -ErrorAction SilentlyContinue

    # Compress-Archive in Windows PowerShell writes backslashes into entry names, so build the zip by hand
    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::Open($out, 'Create')
    Push-Location $stage
    try {
        Get-ChildItem -Recurse -File | ForEach-Object {
            $name = (Resolve-Path -Relative $_.FullName) -replace '^\.\\', '' -replace '\\', '/'
            [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $_.FullName, $name, 'Optimal')
        }
    } finally {
        Pop-Location
        $zip.Dispose()
    }
    $out
} finally {
    Remove-Item $stage -Recurse -Force
}
