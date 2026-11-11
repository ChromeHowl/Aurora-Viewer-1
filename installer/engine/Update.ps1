<#
.SYNOPSIS
    Installs or updates the personal build of the viewer on this PC: checks
    and installs what the build needs (with the user's consent), downloads
    the latest code of the fork, applies the local patches of this PC,
    builds the viewer from source and installs it, with a rollback to the
    previous version when anything fails.

.DESCRIPTION
    Run by the installer (installer/AuroraFD.iss) and by its Start menu
    shortcuts, always through:

      powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File Update.ps1 -Mode <Install|Update|Restore>

    Written for Windows PowerShell 5.1 (the one every Windows 10/11 has).
    A window shows each step in plain words with an elapsed-time clock;
    everything is written to logs\update-<date>.log in the install folder.

    Layout of the install folder (the folder above this script's):
      engine\           this script and settings.json
      viewer\           the installed viewer (the shortcuts point here)
      viewer.previous\  the version before, kept for a rollback
      viewer.new\       a build being installed (only during an update)
      build\src\        the code (a Git clone of the fork, shallow in trees)
      build\target\     Cargo's build folder, kept so that updates are faster
      tools\            Git (MinGit) when this PC had none, and downloads
      logs\             one log per run (the 10 newest are kept)
      state.json        what the installed viewer was built from

    Local patches: every *.patch of %APPDATA%\<folderName>\local-patches is
    applied after the download (git apply --3way, in name order). A patch
    that no longer applies is skipped with a warning; the update goes on.
#>
param(
    [ValidateSet('Install', 'Update', 'Restore')]
    [string]$Mode = 'Update',
    # The install folder. Default: the folder above this script's. (Not a
    # parameter default: $PSScriptRoot is empty there in PowerShell 5.1.)
    [string]$AppDir
)

# Native tools write progress to stderr; they are judged on their exit code.
$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
$EnginePath = $MyInvocation.MyCommand.Path
$EngineDir = Split-Path -Parent $EnginePath
if (-not $AppDir) { $AppDir = Split-Path -Parent $EngineDir }
$AppDir = [IO.Path]::GetFullPath($AppDir)
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[Windows.Forms.Application]::EnableVisualStyles()

$Cfg = Get-Content -LiteralPath (Join-Path $EngineDir 'settings.json') -Raw | ConvertFrom-Json
$Product = $Cfg.productName

$P = [pscustomobject]@{
    Viewer   = Join-Path $AppDir 'viewer'
    Previous = Join-Path $AppDir 'viewer.previous'
    New      = Join-Path $AppDir 'viewer.new'
    Src      = Join-Path $AppDir 'build\src'
    Target   = Join-Path $AppDir 'build\target'
    Tools    = Join-Path $AppDir 'tools'
    Logs     = Join-Path $AppDir 'logs'
    State    = Join-Path $AppDir 'state.json'
    Patches  = Join-Path $env:APPDATA "$($Cfg.folderName)\local-patches"
}
$CargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$Arm64 = ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') -or ($env:PROCESSOR_ARCHITEW6432 -eq 'ARM64')
$UserAgent = "$($Cfg.folderName)-installer"

# Never ask for a password or a token: the fork is public.
$env:GIT_TERMINAL_PROMPT = '0'
$env:GCM_INTERACTIVE = 'never'

# --- Log -----------------------------------------------------------------------

New-Item -ItemType Directory -Force -Path $P.Logs | Out-Null
$Log = Join-Path $P.Logs ('update-{0:yyyyMMdd-HHmmss}.log' -f (Get-Date))
Get-ChildItem -LiteralPath $P.Logs -Filter 'update-*.log' -File -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -Skip 9 |
    ForEach-Object { Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue }

function Write-Log([string]$Text) {
    $line = '[{0:HH:mm:ss}] {1}' -f (Get-Date), $Text
    [IO.File]::AppendAllText($Log, $line + "`r`n")
}

# --- Window ---------------------------------------------------------------------

$Form = New-Object Windows.Forms.Form
$Form.Text = "$Product - $Mode"
$Form.ClientSize = New-Object Drawing.Size(640, 300)
$Form.StartPosition = 'CenterScreen'
$Form.FormBorderStyle = 'FixedDialog'
$Form.MaximizeBox = $false
$Form.Font = New-Object Drawing.Font('Segoe UI', 10)

function New-Label([int]$Y, [int]$H, [float]$Size, [bool]$Bold) {
    $l = New-Object Windows.Forms.Label
    $l.Location = New-Object Drawing.Point(20, $Y)
    $l.Size = New-Object Drawing.Size(600, $H)
    $style = if ($Bold) { [Drawing.FontStyle]::Bold } else { [Drawing.FontStyle]::Regular }
    $l.Font = New-Object Drawing.Font('Segoe UI', $Size, $style)
    $Form.Controls.Add($l)
    $l
}
$StageLabel = New-Label 16 30 13 $true
$DetailLabel = New-Label 52 66 10 $false
$Bar = New-Object Windows.Forms.ProgressBar
$Bar.Location = New-Object Drawing.Point(20, 124)
$Bar.Size = New-Object Drawing.Size(600, 18)
$Bar.Style = 'Marquee'
$Bar.MarqueeAnimationSpeed = 30
$Form.Controls.Add($Bar)
$ClockLabel = New-Label 150 22 9.5 $false
$HintLabel = New-Label 176 66 9.5 $false
$HintLabel.ForeColor = [Drawing.SystemColors]::GrayText

function New-Button([string]$Text, [int]$X) {
    $b = New-Object Windows.Forms.Button
    $b.Text = $Text
    $b.Location = New-Object Drawing.Point($X, 254)
    $b.Size = New-Object Drawing.Size(140, 32)
    $Form.Controls.Add($b)
    $b
}
$LogButton = New-Button 'Open the log' 20
$StartButton = New-Button "Start the viewer" 330
$StartButton.Visible = $false
$CancelButton = New-Button 'Cancel' 480

$script:CancelAsked = $false
$script:Cancelled = $false
$script:Finished = $false
$script:StartAsked = $false
$LogButton.Add_Click({ Start-Process notepad.exe -ArgumentList ('"{0}"' -f $Log) })
$CancelButton.Add_Click({ if ($script:Finished) { $Form.Close() } else { $script:CancelAsked = $true } })
$StartButton.Add_Click({ $script:StartAsked = $true; $Form.Close() })
$Form.Add_FormClosing({
        param($s, $e)
        if (-not $script:Finished) { $e.Cancel = $true; $script:CancelAsked = $true }
    })

$Clock = [Diagnostics.Stopwatch]::StartNew()
$StageClock = [Diagnostics.Stopwatch]::StartNew()

function Format-Span([TimeSpan]$T) {
    if ($T.TotalHours -ge 1) { '{0} h {1:00} min' -f [int][Math]::Floor($T.TotalHours), $T.Minutes }
    elseif ($T.TotalMinutes -ge 1) { '{0} min {1:00} s' -f $T.Minutes, $T.Seconds }
    else { '{0} s' -f $T.Seconds }
}

function Show-Message([string]$Text, [string]$Buttons = 'OK', [string]$Icon = 'Information') {
    [Windows.Forms.MessageBox]::Show($Form, $Text, $Product, $Buttons, $Icon)
}

# Keeps the window alive and the clock ticking; asks before cancelling.
function Update-Ui {
    if (-not $script:Finished) {
        $ClockLabel.Text = 'Elapsed: {0}   (this step: {1})' -f (Format-Span $Clock.Elapsed), (Format-Span $StageClock.Elapsed)
    }
    [Windows.Forms.Application]::DoEvents()
    if ($script:CancelAsked) {
        $script:CancelAsked = $false
        $answer = Show-Message "Stop now? The viewer already installed stays as it is.`n`nWhat was downloaded and built so far is kept: the next update goes faster." 'YesNo' 'Question'
        if ($answer -eq 'Yes') { $script:Cancelled = $true }
    }
}

function Test-Cancelled {
    if ($script:Cancelled) { throw (New-Object OperationCanceledException 'Cancelled by the user.') }
}

function Wait-Ui([int]$Milliseconds) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt $Milliseconds) { Update-Ui; Start-Sleep -Milliseconds 50 }
}

# how the texts name the Update shortcut
$Shortcut = '"Update ' + $Product + '" (Start menu)'
$script:Hint = ''
# One step: a title in plain words, its details, and what to do if it fails.
function Set-Stage([string]$Title, [string]$Detail = '', [string]$Note = '', [string]$IfFails = '') {
    $StageLabel.Text = $Title
    $DetailLabel.Text = $Detail
    $HintLabel.Text = $Note
    $script:Hint = $IfFails.Replace('UPDATE_SHORTCUT', $Shortcut)
    $StageClock.Restart()
    Write-Log "== $Title"
    Update-Ui
}

function Set-Detail([string]$Detail) { $DetailLabel.Text = $Detail; Update-Ui }

# --- Running programs --------------------------------------------------------------

# One argument on a Windows command line (CommandLineToArgvW rules).
function ConvertTo-Arg([string]$Value) {
    if ($Value -and $Value -notmatch '[\s"]') { return $Value }
    '"' + (($Value -replace '(\\*)"', '$1$1\"') -replace '(\\+)$', '$1$1') + '"'
}

# A short command whose output is needed: runs hidden, waits, returns
# Code / Out / Err. The error output goes to the log.
function Invoke-Capture([string]$Exe, [string[]]$Arguments, [string]$InputText = $null, [string]$WorkDir = $AppDir) {
    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = $Exe
    $psi.Arguments = (@($Arguments) | ForEach-Object { ConvertTo-Arg $_ }) -join ' '
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.StandardOutputEncoding = [Text.Encoding]::UTF8
    $psi.StandardErrorEncoding = [Text.Encoding]::UTF8
    # stdin is always closed: a tool waiting for an answer gets none
    $psi.RedirectStandardInput = $true
    try { $proc = [Diagnostics.Process]::Start($psi) }
    catch { Write-Log "cannot start $Exe : $($_.Exception.Message)"; return [pscustomobject]@{ Code = -1; Out = ''; Err = $_.Exception.Message } }
    if ($InputText) { $proc.StandardInput.Write($InputText) }
    $proc.StandardInput.Close()
    $errTask = $proc.StandardError.ReadToEndAsync()
    $out = $proc.StandardOutput.ReadToEnd()
    $proc.WaitForExit()
    $err = $errTask.Result
    $code = $proc.ExitCode
    $proc.Dispose()
    if ($err.Trim()) { Write-Log ("{0} {1}: {2}" -f (Split-Path -Leaf $Exe), ($Arguments -join ' '), $err.Trim()) }
    [pscustomobject]@{ Code = $code; Out = $out; Err = $err }
}

# A long command: its output is appended to the log, the window stays
# alive (clock, Cancel) and $OnTick runs about once a second. Cancel stops
# it, except with -NoKill (an installer that must not be cut off halfway):
# then the step finishes first.
function Invoke-Tool([string]$Exe, [string[]]$Arguments, [string]$WorkDir = $AppDir, [scriptblock]$OnTick = $null, [switch]$NoKill) {
    $parts = @($Exe) + @($Arguments)
    # every part quoted: cmd.exe then reads & | < > ^ ( ) as plain text
    foreach ($a in $parts) { if ($a -match '["%]') { throw "Unsupported character in a command argument: $a" } }
    $line = ($parts | ForEach-Object { '"' + ($_ -replace '(\\+)$', '$1$1') + '"' }) -join ' '
    Write-Log "> $line"
    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = Join-Path $env:SystemRoot 'System32\cmd.exe'
    # /s: cmd removes the outer quotes and keeps the rest as it is
    $psi.Arguments = '/d /s /c "' + $line + ' >> "' + $Log + '" 2>&1"'
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $proc = [Diagnostics.Process]::Start($psi)
    $tick = [Diagnostics.Stopwatch]::StartNew()
    while (-not $proc.WaitForExit(200)) {
        Update-Ui
        if ($OnTick -and $tick.ElapsedMilliseconds -ge 1000) { $tick.Restart(); & $OnTick }
        if ($script:Cancelled -and -not $NoKill) {
            & taskkill.exe /PID $proc.Id /T /F 2>&1 | Out-Null
            $proc.WaitForExit()
            Test-Cancelled
        }
        if ($script:Cancelled -and $NoKill) {
            $HintLabel.Text = 'Stopping after this step: stopping an installer halfway could damage it. Please wait.'
        }
    }
    $proc.WaitForExit()
    $code = $proc.ExitCode
    $proc.Dispose()
    Write-Log "exit code $code"
    $code
}

# What the log gained since the last call (for the build's progress).
$script:LogPos = 0L
function Read-NewLog {
    try {
        $fs = [IO.File]::Open($Log, 'Open', 'Read', 'ReadWrite')
        try {
            if ($fs.Length -le $script:LogPos) { return '' }
            $fs.Position = $script:LogPos
            $buf = New-Object byte[] ([int][Math]::Min(4MB, $fs.Length - $script:LogPos))
            $n = $fs.Read($buf, 0, $buf.Length)
            $script:LogPos += $n
            [Text.Encoding]::UTF8.GetString($buf, 0, $n)
        } finally { $fs.Dispose() }
    } catch { '' }
}
function Get-LogLength { try { (Get-Item -LiteralPath $Log).Length } catch { 0L } }

# --- Downloads and checks --------------------------------------------------------------

function Get-Download([string]$Url, [string]$Dest, [string]$What) {
    if (-not $Url.StartsWith('https://')) { throw "Refused a download that does not use HTTPS: $Url" }
    Write-Log "download $Url"
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Dest) | Out-Null
    $req = [Net.HttpWebRequest]::Create($Url)
    $req.UserAgent = $UserAgent
    $req.Timeout = 60000
    $req.ReadWriteTimeout = 60000
    $resp = $req.GetResponse()
    try {
        if ($resp.ResponseUri.Scheme -ne 'https') { throw "The download of $What left HTTPS." }
        $total = $resp.ContentLength
        $in = $resp.GetResponseStream()
        $out = [IO.File]::Create("$Dest.part")
        try {
            $buf = New-Object byte[] 262144
            $done = 0L
            $tick = [Diagnostics.Stopwatch]::StartNew()
            while (($n = $in.Read($buf, 0, $buf.Length)) -gt 0) {
                $out.Write($buf, 0, $n)
                $done += $n
                if ($tick.ElapsedMilliseconds -ge 250) {
                    $tick.Restart()
                    if ($total -gt 0) { Set-Detail ('Downloading {0}: {1:N0} of {2:N0} MB' -f $What, ($done / 1MB), ($total / 1MB)) }
                    else { Set-Detail ('Downloading {0}: {1:N0} MB' -f $What, ($done / 1MB)) }
                    Test-Cancelled
                }
            }
        } finally { $out.Dispose(); $in.Dispose() }
    } finally { $resp.Dispose() }
    Move-Item -LiteralPath "$Dest.part" -Destination $Dest -Force
}

function Test-Sha256([string]$File, [string]$Expected) {
    $actual = (Get-FileHash -LiteralPath $File -Algorithm SHA256).Hash
    Write-Log "sha256 $File = $actual (expected $Expected)"
    $actual -eq $Expected.ToUpperInvariant()
}

# A valid Authenticode signature whose signer matches $Signer.
function Test-Signed([string]$File, [string]$Signer) {
    $sig = Get-AuthenticodeSignature -LiteralPath $File
    $subject = if ($sig.SignerCertificate) { $sig.SignerCertificate.Subject } else { '' }
    Write-Log "signature of $File : $($sig.Status) $subject"
    ($sig.Status -eq 'Valid') -and ($subject -match $Signer)
}

function Test-Newer([string]$Have, [string]$Need) {
    try { [version]$Have -ge [version]$Need } catch { $false }
}

# --- Prerequisites ---------------------------------------------------------------

function Find-Git {
    $private = Join-Path $P.Tools 'git\cmd\git.exe'
    if (Test-Path -LiteralPath $private) { return $private }
    $cmd = Get-Command git.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($cmd) {
        $r = Invoke-Capture $cmd.Source @('--version')
        if ($r.Code -eq 0 -and $r.Out -match '(\d+\.\d+\.\d+)') {
            if (Test-Newer $Matches[1] $Cfg.minimumGit) { return $cmd.Source }
            Write-Log "Git $($Matches[1]) is older than $($Cfg.minimumGit): a private copy will be used"
        }
    }
    $null
}

function Find-Rustup {
    $cmd = Get-Command rustup.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($cmd) { return $cmd.Source }
    $path = Join-Path $CargoHome 'bin\rustup.exe'
    if (Test-Path -LiteralPath $path) { return $path }
    $null
}

$VsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$VcComponent = 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64'
$VcWorkload = @('--add', 'Microsoft.VisualStudio.Workload.VCTools', '--includeRecommended')

# The MSVC C++ tools (Visual Studio 2022 or newer) and a Windows SDK.
function Test-Msvc {
    if (-not (Test-Path -LiteralPath $VsWhere)) { return $false }
    $r = Invoke-Capture $VsWhere @('-products', '*', '-version', '17.0', '-requires', $VcComponent, '-property', 'installationPath')
    $tools = ($r.Code -eq 0) -and [bool]$r.Out.Trim()
    $sdk = [bool](Get-ChildItem -Path (Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Lib\*\um\x64\kernel32.lib') -ErrorAction SilentlyContinue)
    Write-Log "MSVC tools: $tools, Windows SDK: $sdk"
    $tools -and $sdk
}

function Install-Git {
    Set-Stage 'Installing Git...' 'Asking GitHub for the latest Git for Windows (MinGit, a private copy for this program only).' '' 'Check the internet connection, then run UPDATE_SHORTCUT again.'
    # -UserAgent: PowerShell 5.1 refuses that header in -Headers
    $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/git-for-windows/git/releases/latest' -UserAgent $UserAgent -UseBasicParsing
    $pattern = '^MinGit-[0-9.]+-64-bit\.zip$'
    $asset = @($release.assets | Where-Object { $_.name -match $pattern }) | Select-Object -First 1
    if (-not $asset) { throw "No MinGit package in Git for Windows $($release.tag_name)." }
    $url = [string]$asset.browser_download_url
    if (-not $url.StartsWith('https://github.com/git-for-windows/git/releases/download/')) { throw "Unexpected Git download address: $url" }
    $zip = Join-Path $P.Tools "downloads\$($asset.name)"
    Get-Download $url $zip 'Git'
    $digest = [string]$asset.digest
    $hashed = $false
    if ($digest -match '^sha256:([0-9a-fA-F]{64})$') {
        if (-not (Test-Sha256 $zip $Matches[1])) { Remove-Item -LiteralPath $zip -Force; throw 'The Git download is damaged (its SHA-256 does not match GitHub''s).' }
        $hashed = $true
    }
    $tmp = Join-Path $P.Tools 'git.new'
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Recurse -Force }
    Set-Detail 'Unpacking Git...'
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::ExtractToDirectory($zip, $tmp)
    $signed = Test-Signed (Join-Path $tmp 'cmd\git.exe') 'Johannes Schindelin'
    if (-not $hashed -and -not $signed) {
        Remove-Item -LiteralPath $tmp -Recurse -Force
        throw 'Git could not be verified (no published checksum and no valid signature).'
    }
    $final = Join-Path $P.Tools 'git'
    if (Test-Path -LiteralPath $final) { Remove-Item -LiteralPath $final -Recurse -Force }
    Move-Item -LiteralPath $tmp -Destination $final
    Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue
}

function Install-Msvc {
    $ifFails = 'Install "Visual Studio 2022 Build Tools" with "Desktop development with C++" from visualstudio.microsoft.com, then run UPDATE_SHORTCUT again.'
    Set-Stage 'Installing the Visual Studio C++ build tools...' 'Windows will ask for permission: click Yes. The Visual Studio Installer then shows its own progress; this takes 10-30 minutes.' 'This is the largest download (several GB).' $ifFails
    # Build Tools already there, without the C++ tools: add them to it.
    if (Test-Path -LiteralPath $VsWhere) {
        $bt = Invoke-Capture $VsWhere @('-products', 'Microsoft.VisualStudio.Product.BuildTools', '-version', '17.0', '-property', 'installationPath')
        $path = ($bt.Out -split "`r?`n" | Where-Object { $_.Trim() } | Select-Object -First 1)
        $setup = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\setup.exe'
        if ($path -and (Test-Path -LiteralPath $setup)) {
            [void](Invoke-Tool $setup (@('modify', '--installPath', $path.Trim()) + $VcWorkload + @('--passive', '--norestart')) -NoKill)
            Test-Cancelled
            if (Test-Msvc) { return }
        }
    }
    if (Get-Command winget.exe -ErrorAction SilentlyContinue) {
        $override = (@('--passive', '--wait', '--norestart') + $VcWorkload) -join ' '
        [void](Invoke-Tool 'winget.exe' @('install', '--id', 'Microsoft.VisualStudio.2022.BuildTools', '--exact', '--source', 'winget',
                '--accept-package-agreements', '--accept-source-agreements', '--override', $override) -NoKill)
        Test-Cancelled
        if (Test-Msvc) { return }
        Write-Log 'winget did not install the C++ tools: using the official bootstrapper'
    }
    $exe = Join-Path $P.Tools 'downloads\vs_BuildTools.exe'
    Get-Download 'https://aka.ms/vs/17/release/vs_BuildTools.exe' $exe 'the Visual Studio Build Tools installer'
    if (-not (Test-Signed $exe 'O=Microsoft Corporation')) { Remove-Item -LiteralPath $exe -Force; throw 'The Visual Studio installer is not signed by Microsoft: it was not run.' }
    Set-Detail 'Windows will ask for permission: click Yes. The Visual Studio Installer then shows its own progress.'
    $code = Invoke-Tool $exe (@('--passive', '--wait', '--norestart') + $VcWorkload) -NoKill
    Test-Cancelled
    Remove-Item -LiteralPath $exe -Force -ErrorAction SilentlyContinue
    if ($code -eq 3010) { Write-Log 'Visual Studio asks for a restart later' }
    if (-not (Test-Msvc)) { throw "The Visual Studio C++ build tools are still missing (installer exit code $code)." }
}

function Install-Rust {
    Set-Stage 'Installing Rust...' 'Downloading the official Rust installer (rustup) from static.rust-lang.org.' '' 'Check the internet connection, then run UPDATE_SHORTCUT again.'
    $url = 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe'
    $exe = Join-Path $P.Tools 'downloads\rustup-init.exe'
    Get-Download $url $exe 'Rust'
    Get-Download "$url.sha256" "$exe.sha256" 'Rust checksum'
    $expected = ((Get-Content -LiteralPath "$exe.sha256" -Raw) -split '\s+')[0]
    if ($expected -notmatch '^[0-9a-fA-F]{64}$' -or -not (Test-Sha256 $exe $expected)) {
        Remove-Item -LiteralPath $exe, "$exe.sha256" -Force -ErrorAction SilentlyContinue
        throw 'The Rust installer is damaged (its SHA-256 does not match).'
    }
    Set-Detail 'Installing Rust for your Windows account (no administrator rights needed)...'
    [void](Test-Signed $exe 'Rust')  # logged only: the checksum above is the check
    $code = Invoke-Tool $exe @('-y', '--default-toolchain', 'stable', '--profile', 'minimal') -NoKill
    Test-Cancelled
    Remove-Item -LiteralPath $exe, "$exe.sha256" -Force -ErrorAction SilentlyContinue
    if ($code -ne 0 -or -not (Find-Rustup)) { throw "The Rust installation failed (exit code $code)." }
}

# The newest stable Rust, at least the version the code needs.
function Update-Rust([string]$Rustup) {
    $need = $Cfg.minimumRust
    foreach ($toml in @((Join-Path $P.Src 'Cargo.toml'), (Join-Path $P.Src 'crates\aurora-viewer\Cargo.toml'))) {
        if ((Test-Path -LiteralPath $toml) -and ((Get-Content -LiteralPath $toml -Raw) -match '(?m)^\s*rust-version\s*=\s*"([0-9.]+)"')) {
            if (-not (Test-Newer $need $Matches[1])) { $need = $Matches[1] }
        }
    }
    Set-Stage 'Updating Rust...' "The code needs Rust $need or newer: getting the newest stable version." '' 'Check the internet connection, then run UPDATE_SHORTCUT again.'
    $code = Invoke-Tool $Rustup @('toolchain', 'install', 'stable', '--profile', 'minimal')
    if ($code -eq 0) { $code = Invoke-Tool $Rustup @('update', 'stable', '--no-self-update') }
    $r = Invoke-Capture $Rustup @('run', 'stable', 'rustc', '--version')
    $have = if ($r.Out -match 'rustc (\d+\.\d+\.\d+)') { $Matches[1] } else { '0.0.0' }
    Write-Log "rustc $have (needs $need)"
    if (-not (Test-Newer $have $need)) {
        throw "Rust $have is installed, but the code needs $need or newer, and the update failed (exit code $code)."
    }
}

# --- Code -----------------------------------------------------------------------

function Invoke-Git([string[]]$Arguments, [string]$InputText = $null) {
    Invoke-Capture $script:Git (@('-C', $P.Src) + $Arguments) $InputText
}

# The fork's branch, with only the commits up front (--filter=tree:0): the
# files of the checked-out commit are downloaded at checkout.
function Get-Source {
    Set-Stage 'Downloading the latest code...' "From $($Cfg.repository) (branch $($Cfg.branch))." '' 'Check the internet connection, then run UPDATE_SHORTCUT again.'
    $branchRef = "refs/remotes/origin/$($Cfg.branch)"
    if (-not (Test-Path -LiteralPath (Join-Path $P.Src '.git'))) {
        if (Test-Path -LiteralPath $P.Src) { Remove-Item -LiteralPath $P.Src -Recurse -Force }
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $P.Src) | Out-Null
        $code = Invoke-Tool $script:Git @('-c', 'core.longpaths=true', 'clone', '--filter=tree:0', '--no-checkout', '--no-tags',
            '--single-branch', '--branch', $Cfg.branch, $Cfg.repository, $P.Src)
        if ($code -ne 0) { throw 'The code could not be downloaded.' }
        foreach ($kv in @(@('core.longpaths', 'true'), @('core.autocrlf', 'false'), @('advice.detachedHead', 'false'))) {
            [void](Invoke-Git @('config', $kv[0], $kv[1]))
        }
    } else {
        [void](Invoke-Git @('remote', 'set-url', 'origin', $Cfg.repository))
        $code = Invoke-Tool $script:Git @('-C', $P.Src, 'fetch', '--no-tags', '--prune', 'origin',
            "+refs/heads/$($Cfg.branch):$branchRef")
        if ($code -ne 0) { throw 'The latest code could not be downloaded.' }
    }
    $sha = (Invoke-Git @('rev-parse', $branchRef)).Out.Trim()
    if ($sha -notmatch '^[0-9a-f]{40}$') { throw "The downloaded branch has no valid commit ($sha)." }
    $sha
}

# The fork has no release tags of its own: the version number comes from
# the official tags (git describe). Only their names are read
# (ls-remote); each becomes a local tag on the same commit, when the fork
# has that commit. Nothing else is downloaded.
function Set-VersionTags([string]$Sha) {
    Set-Detail 'Reading the version numbers...'
    $r = Invoke-Capture $script:Git @('ls-remote', '--tags', $Cfg.versionTags)
    if ($r.Code -ne 0) { Write-Log 'WARNING: the version tags could not be read; the viewer will show version 0.0.0'; return }
    $known = New-Object 'System.Collections.Generic.HashSet[string]'
    foreach ($c in ((Invoke-Git @('rev-list', $Sha)).Out -split "`n")) { if ($c.Trim()) { [void]$known.Add($c.Trim()) } }
    $tags = @{}
    foreach ($line in ($r.Out -split "`n")) {
        if ($line -match '^([0-9a-f]{40})\s+refs/tags/(v[0-9]+\.[0-9]+\.[0-9]+[0-9A-Za-z.+-]*?)(\^\{\})?\s*$') {
            # an annotated tag's peeled line (^{}) names its commit
            if ($Matches[3] -or -not $tags.ContainsKey($Matches[2])) { $tags[$Matches[2]] = $Matches[1] }
        }
    }
    $lines = foreach ($name in $tags.Keys) { if ($known.Contains($tags[$name])) { "update refs/tags/$name $($tags[$name])" } }
    if ($lines) {
        $u = Invoke-Git @('update-ref', '--stdin') ((@($lines) -join "`n") + "`n")
        if ($u.Code -ne 0) { Write-Log 'WARNING: the version tags could not be written' }
    }
    Write-Log ("{0} version tags on this branch" -f @($lines | Where-Object { $_ }).Count)
}

function Get-PatchFiles {
    if (-not (Test-Path -LiteralPath $P.Patches)) { return @() }
    @(Get-ChildItem -LiteralPath $P.Patches -Filter '*.patch' -File -ErrorAction SilentlyContinue | Sort-Object Name)
}

function Set-Checkout([string]$Sha) {
    Set-Stage 'Downloading the latest code...' 'Getting the files of the latest version.' '' 'Check the internet connection, then run UPDATE_SHORTCUT again.'
    $code = Invoke-Tool $script:Git @('-C', $P.Src, '-c', 'core.longpaths=true', 'checkout', '--force', '--detach', $Sha)
    if ($code -ne 0) { throw 'The files of the latest version could not be downloaded.' }
    [void](Invoke-Git @('clean', '-ffdxq'))
}

# Each local patch on its own commit; one that no longer applies is skipped.
function Add-LocalPatches {
    $applied = @(); $skipped = @()
    $patches = Get-PatchFiles
    if (-not $patches) { return [pscustomobject]@{ Applied = $applied; Skipped = $skipped } }
    Set-Stage 'Applying your local patches...' ("{0} patch(es) from {1}" -f $patches.Count, $P.Patches)
    foreach ($f in $patches) {
        Set-Detail "Applying $($f.Name)..."
        $code = Invoke-Tool $script:Git @('-C', $P.Src, 'apply', '--3way', '--whitespace=nowarn', $f.FullName)
        $ok = $false
        if ($code -eq 0) {
            [void](Invoke-Git @('add', '-A'))
            $c = Invoke-Git @('-c', 'user.name=Local patches', '-c', 'user.email=local-patches@localhost', '-c', 'commit.gpgsign=false',
                'commit', '-q', '--no-verify', '-m', "Local patch $($f.Name)")
            $ok = ($c.Code -eq 0)
        }
        if ($ok) { $applied += $f.Name; Write-Log "patch applied: $($f.Name)" }
        else {
            Write-Log "WARNING: patch skipped (it no longer applies): $($f.Name)"
            [void](Invoke-Git @('reset', '-q', '--hard', 'HEAD'))
            [void](Invoke-Git @('clean', '-ffdq'))
            $skipped += $f.Name
        }
    }
    [pscustomobject]@{ Applied = $applied; Skipped = $skipped }
}

# What the installed viewer was built from: the commit, the patches (name
# and content), the label and this script.
function Get-Fingerprint([string]$Sha) {
    $parts = @($Sha, $Cfg.buildLabel, (Get-FileHash -LiteralPath $EnginePath -Algorithm SHA256).Hash)
    foreach ($f in Get-PatchFiles) { $parts += "$($f.Name)=$((Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash)" }
    $parts -join '|'
}

function Read-State {
    if (-not (Test-Path -LiteralPath $P.State)) { return $null }
    try { Get-Content -LiteralPath $P.State -Raw | ConvertFrom-Json } catch { $null }
}

function Write-State($State) {
    [IO.File]::WriteAllText($P.State, ($State | ConvertTo-Json), (New-Object Text.UTF8Encoding $false))
}

# --- Build ----------------------------------------------------------------------

function Get-RamGB {
    try { [Math]::Round((Get-CimInstance Win32_ComputerSystem -ErrorAction Stop).TotalPhysicalMemory / 1GB, 1) } catch { 0 }
}

function Get-FreeGB([string]$Path) {
    try { [Math]::Round((New-Object IO.DriveInfo ([IO.Path]::GetPathRoot($Path))).AvailableFreeSpace / 1GB, 1) } catch { 999 }
}

$script:Compiled = 0
$script:Current = ''
$script:Quiet = [Diagnostics.Stopwatch]::StartNew()
$BuildTick = {
    $text = Read-NewLog
    if ($text) {
        $found = [regex]::Matches($text, '(?m)^\s+Compiling (\S+)')
        if ($found.Count) {
            $script:Compiled += $found.Count
            $script:Current = $found[$found.Count - 1].Groups[1].Value
            $script:Quiet.Restart()
        }
    }
    $detail = 'Parts compiled so far: {0}.' -f $script:Compiled
    if ($script:Current) { $detail += "  Now: $($script:Current)" }
    $DetailLabel.Text = $detail
    if ($script:Current -eq 'aurora-viewer' -and $script:Quiet.Elapsed.TotalSeconds -ge 60) {
        $HintLabel.Text = 'Now compiling and linking the viewer itself: the longest step (10-40 minutes), and it shows no progress. Your PC is working - leave this window open.'
    }
}

function Invoke-Build([bool]$LowMemory) {
    $env:AURORA_PERSONAL_BUILD = $Cfg.buildLabel
    $env:CARGO_TARGET_DIR = $P.Target
    $env:CARGO_TERM_COLOR = 'never'
    # a release tag from elsewhere must not stamp this build
    Remove-Item Env:AURORA_VERSION_TAG -ErrorAction SilentlyContinue
    Remove-Item Env:AURORA_UPDATE_TEST_VERSION -ErrorAction SilentlyContinue
    $cargoArgs = @('build', '--release', '--locked', '-p', 'aurora-viewer')
    if ($LowMemory) {
        # far less memory at the final link, a little less optimised
        $env:CARGO_PROFILE_RELEASE_LTO = 'off'
        $cargoArgs += @('-j', '2')
    } else {
        Remove-Item Env:CARGO_PROFILE_RELEASE_LTO -ErrorAction SilentlyContinue
    }
    $script:Compiled = 0
    $script:Current = ''
    $script:LogPos = Get-LogLength
    $start = $script:LogPos
    $code = Invoke-Tool (Join-Path $CargoHome 'bin\cargo.exe') $cargoArgs $P.Src $BuildTick
    $script:LogPos = $start
    $output = Read-NewLog
    [pscustomobject]@{ Code = $code; CodeError = ($output -match 'error\[E\d+\]') }
}

function Build-Viewer {
    $ram = Get-RamGB
    $low = ($ram -gt 0 -and $ram -lt 8)
    Write-Log "memory: $ram GB; low-memory mode from the start: $low"
    $note = 'Your PC is working. The first build takes the longest; later updates only rebuild what changed. You can use your PC meanwhile.'
    $ifFails = 'Run UPDATE_SHORTCUT again later: a fix may be on its way. Closing other programs (games, browsers) frees memory for the build.'
    Set-Stage 'Building - this can take 20-40 minutes, your PC is working' 'Starting the compiler...' $note $ifFails
    $r = Invoke-Build $low
    if ($r.Code -ne 0 -and -not $low -and -not $r.CodeError) {
        Write-Log 'the build failed without a code error: trying again in the low-memory mode'
        Set-Stage 'Building again, in the low-memory mode' 'The first attempt failed, most likely for lack of memory. This attempt uses less memory and goes a bit slower.' $note $ifFails
        $r = Invoke-Build $true
    }
    if ($r.Code -ne 0) {
        if ($r.CodeError) { throw 'This version of the code does not build (a compile error). The installed viewer is unchanged.' }
        throw 'The build failed. The installed viewer is unchanged.'
    }
}

# --- Install --------------------------------------------------------------------

function Test-ViewerRunning {
    $prefix = $P.Viewer.TrimEnd('\') + '\'
    $procs = Get-CimInstance Win32_Process -Filter "Name='aurora-viewer.exe'" -ErrorAction SilentlyContinue
    [bool](@($procs) | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) })
}

function Wait-ViewerClosed {
    while (Test-ViewerRunning) {
        $a = Show-Message "$Product is open. Close it (log out first), then click Retry to finish the update." 'RetryCancel' 'Warning'
        if ($a -ne 'Retry') { throw (New-Object OperationCanceledException 'The viewer was still open.') }
    }
}

function Get-ViewerVersion([string]$Dir) {
    $exe = Join-Path $Dir 'aurora-viewer.exe'
    if (-not (Test-Path -LiteralPath $exe)) { return $null }
    $r = Invoke-Capture $exe @('--version')
    if ($r.Code -eq 0 -and $r.Out.Trim() -match '^Aurora Viewer (\S+)$') { return $Matches[1] }
    $null
}

function New-Package([string]$Sha, $Patches) {
    Set-Stage 'Preparing the new version...' 'Putting the viewer and its files together.' '' 'Run UPDATE_SHORTCUT again.'
    if (Test-Path -LiteralPath $P.New) { Remove-Item -LiteralPath $P.New -Recurse -Force }
    $ps = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $code = Invoke-Tool $ps @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $P.Src 'scripts\package.ps1'),
        '-Exe', (Join-Path $P.Target 'release\aurora-viewer.exe'), '-Out', $P.New, '-Source', $P.Src)
    $version = Get-ViewerVersion $P.New
    if ($code -ne 0 -or -not $version) { throw 'The new version could not be prepared.' }
    $lines = @(
        "$Product $version (Aurora Viewer $version, a personal build)",
        "Built on this PC from $($Cfg.repository) $($Cfg.branch) at $($Sha.Substring(0, 9))",
        "built on $(Get-Date -Format 'yyyy-MM-dd HH:mm')"
    )
    if ($Patches -and ($Patches.Applied.Count -or $Patches.Skipped.Count)) {
        $lines += "local patches applied: $($Patches.Applied -join ', ')"
        if ($Patches.Skipped.Count) { $lines += "local patches skipped: $($Patches.Skipped -join ', ')" }
    }
    [IO.File]::WriteAllLines((Join-Path $P.New 'version.txt'), [string[]]$lines)
    $version
}

# One folder rename on the same disk (atomic: never half copied). Throws a
# clear error naming both folders.
function Move-Folder([string]$From, [string]$To) {
    try { [IO.Directory]::Move($From, $To) }
    catch { throw "Could not rename $From to $To ($($_.Exception.InnerException.Message))." }
    Write-Log "renamed $From -> $To"
}

# A folder no longer needed: renamed aside first (so that a locked file never
# leaves it half there under its old name), then deleted as far as possible.
# What cannot be deleted now is retried at the next run (Clear-Leftovers).
function Remove-Aside([string]$Dir) {
    if (-not (Test-Path -LiteralPath $Dir)) { return }
    $aside = Join-Path $AppDir ('old-{0:yyyyMMddHHmmss}-{1}' -f (Get-Date), (Split-Path -Leaf $Dir))
    try { [IO.Directory]::Move($Dir, $aside) } catch { Write-Log "WARNING: could not set $Dir aside: $($_.Exception.InnerException.Message)"; return }
    Remove-Item -LiteralPath $aside -Recurse -Force -ErrorAction SilentlyContinue
}

function Clear-Leftovers {
    Get-ChildItem -LiteralPath $AppDir -Directory -Filter 'old-*' -ErrorAction SilentlyContinue |
        ForEach-Object { Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue }
}

# viewer.new becomes viewer, the old viewer becomes viewer.previous, and the
# version before that is only deleted once the new one has started
# (--version). Every step is a folder rename; if one fails, the folders are
# put back as they were and the first error is kept.
function Install-New {
    Set-Stage 'Installing the new version...' '' '' 'The previous version was put back. Run UPDATE_SHORTCUT again.'
    Wait-ViewerClosed
    $older = Join-Path $AppDir 'viewer.older'
    if (Test-Path -LiteralPath $older) { Remove-Aside $older }
    $hadOld = Test-Path -LiteralPath $P.Viewer
    $hadPrevious = Test-Path -LiteralPath $P.Previous
    $done = @()
    try {
        if ($hadPrevious -and $hadOld) { Move-Folder $P.Previous $older; $done += 'older' }
        if ($hadOld) { Move-Folder $P.Viewer $P.Previous; $done += 'previous' }
        Move-Folder $P.New $P.Viewer; $done += 'viewer'
        $version = Get-ViewerVersion $P.Viewer
        if (-not $version) { throw 'The new version does not start.' }
    } catch {
        $first = $_.Exception.Message
        Write-Log "install failed: $first; putting the previous version back"
        $problems = @()
        if ($done -contains 'viewer') {
            try { Move-Folder $P.Viewer (Join-Path $AppDir ('old-{0:yyyyMMddHHmmss}-failed' -f (Get-Date))) } catch { $problems += $_.Exception.Message }
        }
        if ($done -contains 'previous' -and -not (Test-Path -LiteralPath $P.Viewer)) {
            try { Move-Folder $P.Previous $P.Viewer } catch { $problems += $_.Exception.Message }
        }
        if ($done -contains 'older' -and -not (Test-Path -LiteralPath $P.Previous)) {
            try { Move-Folder $older $P.Previous } catch { $problems += $_.Exception.Message }
        }
        if ($problems) {
            throw "$first Putting the previous version back also failed: $($problems -join ' ') Your previous version is in $AppDir (folders viewer.previous or viewer.older); run UPDATE_SHORTCUT again once the PC has restarted.".Replace('UPDATE_SHORTCUT', $Shortcut)
        }
        throw $first
    }
    Write-Log "installed $version"
    if (Test-Path -LiteralPath $older) { Remove-Aside $older }
    $version
}

# viewer and viewer.previous swap places, by renames; a failure puts them
# back.
function Restore-Previous {
    Set-Stage 'Going back to the previous version...' '' '' 'Run UPDATE_SHORTCUT to build the latest version again.'
    if (-not (Test-Path -LiteralPath (Join-Path $P.Previous 'aurora-viewer.exe'))) {
        Show-Message 'There is no previous version to go back to.' | Out-Null
        return $null
    }
    $old = Get-ViewerVersion $P.Previous
    if ((Show-Message "Go back to the previous version ($old)?" 'YesNo' 'Question') -ne 'Yes') { return $null }
    Wait-ViewerClosed
    $swap = Join-Path $AppDir 'viewer.swap'
    if (Test-Path -LiteralPath $swap) { Remove-Aside $swap }
    $hadViewer = Test-Path -LiteralPath $P.Viewer
    if ($hadViewer) { Move-Folder $P.Viewer $swap }
    try { Move-Folder $P.Previous $P.Viewer }
    catch {
        if ($hadViewer) { Move-Folder $swap $P.Viewer }
        throw
    }
    if ($hadViewer) { Move-Folder $swap $P.Previous }
    # the next update rebuilds the latest version
    $state = Read-State
    if ($state) { $state.fingerprint = ''; $state.version = $old; Write-State $state }
    $old
}

# --- Main -----------------------------------------------------------------------

function Invoke-Main {
    # cmd.exe would expand %...% in the paths it is given
    if ($AppDir -match '[%"]') { throw "The install folder's name contains % or a double quote: reinstall into a folder without them." }
    Clear-Leftovers
    if ($Mode -eq 'Restore') { return Restore-Previous }

    # 1. What this PC needs
    Set-Stage 'Checking this PC...' 'Looking for Git, Rust and the Visual Studio C++ build tools.'
    if ($Arm64) { throw 'This PC runs Windows on ARM: the build supports x64 PCs only.' }
    $script:Git = Find-Git
    $rustup = Find-Rustup
    $msvc = Test-Msvc
    $missing = @()
    if (-not $msvc) { $missing += '- Visual Studio 2022 Build Tools, C++ tools and Windows SDK (Microsoft; about 6-8 GB on disk; asks for administrator permission)' }
    if (-not $rustup) { $missing += '- Rust (rustup and the stable toolchain, from rust-lang.org; about 1 GB, for your account only)' }
    if (-not $script:Git) { $missing += '- Git (MinGit from git-for-windows on GitHub; about 50 MB, private to this program)' }
    $free = Get-FreeGB $AppDir
    $firstBuild = -not (Test-Path -LiteralPath $P.Target)
    Write-Log "free space: $free GB; first build: $firstBuild"
    if ($missing) {
        $text = "To build $Product on this PC, these free tools from their official websites are needed:`n`n" + ($missing -join "`n") +
        "`n`nBefore each build, Rust's stable version is brought up to date (rustup update stable): the code needs a recent one." +
        "`n`nThen the code (about 150 MB) is downloaded and built: this needs about $($Cfg.diskNeededGB) GB of free disk space and 20-60 minutes the first time.`n`nInstall them now?"
        if ((Show-Message $text 'YesNo' 'Question') -ne 'Yes') { throw (New-Object OperationCanceledException 'The tools were not installed.') }
    }
    if ($firstBuild -and $free -lt $Cfg.diskNeededGB) {
        $a = Show-Message "Only $free GB is free on the disk of $AppDir; the first build needs about $($Cfg.diskNeededGB) GB. Free some space first.`n`nTry anyway?" 'YesNo' 'Warning'
        if ($a -ne 'Yes') { throw (New-Object OperationCanceledException 'Not enough disk space.') }
    }
    if (-not $msvc) { Install-Msvc; Test-Cancelled }
    if (-not $rustup) { Install-Rust; $rustup = Find-Rustup; Test-Cancelled }
    if (-not $script:Git) { Install-Git; $script:Git = Find-Git; Test-Cancelled }
    if (-not $script:Git) { throw 'Git is still missing.' }
    $env:PATH = (Split-Path -Parent $script:Git) + ';' + (Join-Path $CargoHome 'bin') + ';' + $env:PATH

    # 2. The code
    $sha = Get-Source
    Test-Cancelled
    $fingerprint = Get-Fingerprint $sha
    $state = Read-State
    $installed = Get-ViewerVersion $P.Viewer
    if ($installed -and $state -and $state.fingerprint -eq $fingerprint) {
        Set-Stage "$Product is already up to date" "Version $installed, built on $($state.builtAt)."
        if ($Mode -eq 'Install') { return $installed }
        if ((Show-Message "$Product is already up to date (version $installed).`n`nBuild it again anyway?" 'YesNo' 'Question') -ne 'Yes') { return $installed }
    }
    Set-Checkout $sha
    Set-VersionTags $sha
    $patches = Add-LocalPatches
    Test-Cancelled

    # 3. Build, package, install
    Update-Rust $rustup
    Test-Cancelled
    Build-Viewer
    $version = New-Package $sha $patches
    $version = Install-New
    Write-State ([pscustomobject]@{
            schema      = 1
            fingerprint = $fingerprint
            version     = $version
            commit      = $sha
            builtAt     = (Get-Date -Format 'yyyy-MM-dd HH:mm')
        })
    if ($patches.Skipped.Count) {
        $text = "These local patches no longer apply to the latest code and were left out:`n`n" + ($patches.Skipped -join "`n") +
        "`n`nThe update went on without them. Update the patches, then run $Shortcut again."
        Show-Message $text 'OK' 'Warning' | Out-Null
    }
    $version
}

$created = $false
$mutex = New-Object Threading.Mutex($true, "Local\$($Cfg.folderName)-update", [ref]$created)
if (-not $created) {
    [Windows.Forms.MessageBox]::Show("$Product is already being installed or updated in another window.", $Product) | Out-Null
    exit 1
}

$exitCode = 0
Write-Log "$Product - mode $Mode - install folder $AppDir"
Write-Log ("Windows {0}, PowerShell {1}" -f [Environment]::OSVersion.Version, $PSVersionTable.PSVersion)
# in front of Setup's window, which waits behind it
$Form.TopMost = $true
$Form.Show()
$Form.Activate()
$Form.TopMost = $false
try {
    $version = Invoke-Main
    $Bar.Style = 'Continuous'
    $Bar.Value = 100
    if ($version) {
        $StageLabel.Text = "Done: $Product $version is installed."
        $DetailLabel.Text = 'Start it from the Start menu or the desktop. To get the latest version later, use "Update ' + $Product + '" in the Start menu.'
        $StartButton.Visible = Test-Path -LiteralPath (Join-Path $P.Viewer 'aurora-viewer.exe')
    } else {
        $StageLabel.Text = 'Nothing was changed.'
        $DetailLabel.Text = ''
    }
    $HintLabel.Text = ''
} catch [OperationCanceledException] {
    $exitCode = 2
    Write-Log "stopped: $($_.Exception.Message)"
    $Bar.Style = 'Continuous'; $Bar.Value = 0
    $StageLabel.Text = 'Stopped. The installed viewer is unchanged.'
    $DetailLabel.Text = $_.Exception.Message
    $HintLabel.Text = 'Run "Update ' + $Product + '" from the Start menu to try again.'
} catch {
    $exitCode = 1
    Write-Log "ERROR: $($_.Exception.Message)"
    Write-Log ($_.ScriptStackTrace)
    # a half-installed new version never stays
    if (Test-Path -LiteralPath $P.New) { Remove-Item -LiteralPath $P.New -Recurse -Force -ErrorAction SilentlyContinue }
    $Bar.Style = 'Continuous'; $Bar.Value = 0
    $StageLabel.Text = 'Something went wrong.'
    $DetailLabel.Text = $_.Exception.Message
    $HintLabel.Text = ("What to do: {0}`nThe log file has the details: {1}" -f $(if ($script:Hint) { $script:Hint } else { "Run $Shortcut again." }), $Log)
    $HintLabel.ForeColor = [Drawing.SystemColors]::ControlText
}
$script:Finished = $true
$CancelButton.Text = 'Close'
$ClockLabel.Text = 'Total time: {0}' -f (Format-Span $Clock.Elapsed)
Write-Log "finished with code $exitCode after $(Format-Span $Clock.Elapsed)"
while ($Form.Visible) { [Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 50 }
if ($script:StartAsked) { Start-Process -FilePath (Join-Path $P.Viewer 'aurora-viewer.exe') -WorkingDirectory $P.Viewer }
$mutex.ReleaseMutex()
$mutex.Dispose()
exit $exitCode
