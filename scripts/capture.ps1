# Capture a window screenshot of MasseyCode for the documentation.
#
#   powershell -ExecutionPolicy Bypass -File scripts/capture.ps1 `
#       -Out docs/screenshots/overview.png -W 1400 -H 900
#
# The window is moved to the top-left corner, made topmost and captured with
# GDI.  Keys are posted as WM_KEYDOWN/WM_KEYUP after the window has settled,
# which is enough for a deterministic screenshot even though the window is not
# focused.  Arguments:
#
#   -Exe      binary to run (default: target\debug\masseycode.exe)
#   -Out      PNG to write
#   -W -H     window size in physical pixels
#   -AppArgs  arguments passed to MasseyCode, e.g. "--asm"
#   -Keys     comma separated virtual-key codes in hex, e.g. "7A,7A" for F11,F11
#   -Settle   milliseconds to wait after resizing and after the last key
#
# Captures one window per invocation; the process is terminated at the end.
# Run it from the repository root so the relative example paths work.

param(
    [string]$Exe = "target\debug\masseycode.exe",
    [string]$Out = "docs\screenshots\shot.png",
    [int]$W = 1400,
    [int]$H = 900,
    [string]$AppArgs = "",
    [string]$Keys = "",
    [int]$Settle = 1600,
    [int]$KeyDelay = 160
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class Shot {
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lp);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint msg, IntPtr wp, IntPtr lp);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool repaint);
  public delegate bool EnumProc(IntPtr h, IntPtr lp);
  public struct RECT { public int L, T, R, B; }
  public struct POINT { public int X, Y; }
  public static string Class(IntPtr h) { var sb = new StringBuilder(256); GetClassNameW(h, sb, 256); return sb.ToString(); }
}
"@
[Shot]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null

# A previous run that was interrupted leaves a window behind; the capture would
# then photograph the wrong process.
Get-Process masseycode -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300

$startArgs = @{ FilePath = $Exe; WorkingDirectory = (Get-Location); PassThru = $true }
if ($AppArgs) { $startArgs.ArgumentList = $AppArgs }
$proc = Start-Process @startArgs

# Wait for the first large, visible top-level window that belongs to us; tooltips
# and hidden helper windows are small, the real window is not.
$script:best = [IntPtr]::Zero
$script:bestArea = 0
$hwnd = [IntPtr]::Zero
$deadline = (Get-Date).AddSeconds(25)
while ((Get-Date) -lt $deadline -and $hwnd -eq [IntPtr]::Zero) {
    Start-Sleep -Milliseconds 400
    $script:best = [IntPtr]::Zero
    $script:bestArea = 0
    [Shot]::EnumWindows({
        param($h, $lp)
        $owner = 0
        [Shot]::GetWindowThreadProcessId($h, [ref]$owner) | Out-Null
        if ($owner -eq $proc.Id -and [Shot]::IsWindowVisible($h)) {
            if ([Shot]::Class($h) -ne "tooltips_class32") {
                $r = New-Object Shot+RECT
                [Shot]::GetWindowRect($h, [ref]$r) | Out-Null
                $area = ($r.R - $r.L) * ($r.B - $r.T)
                if ($area -gt $script:bestArea) {
                    $script:bestArea = $area
                    $script:best = $h
                }
            }
        }
        return $true
    }, [IntPtr]::Zero) | Out-Null
    if ($script:bestArea -gt 200000) { $hwnd = $script:best }
}
if ($hwnd -eq [IntPtr]::Zero) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    throw "no MasseyCode window appeared"
}

# Pin to the top-left corner, on top, at exactly the requested size.
[Shot]::SetWindowPos($hwnd, [IntPtr](-1), 0, 0, 0, 0, 0x0001 -bor 0x0040) | Out-Null
[Shot]::MoveWindow($hwnd, 0, 0, $W, $H, $true) | Out-Null
Start-Sleep -Milliseconds $Settle

foreach ($key in ($Keys -split ',')) {
    if ($key.Trim() -eq "") { continue }
    $vk = [Convert]::ToInt32($key.Trim(), 16)
    [Shot]::PostMessageW($hwnd, 0x0100, [IntPtr]$vk, [IntPtr]0) | Out-Null
    [Shot]::PostMessageW($hwnd, 0x0101, [IntPtr]$vk, [IntPtr]0) | Out-Null
    Start-Sleep -Milliseconds $KeyDelay
}
Start-Sleep -Milliseconds $Settle

$rect = New-Object Shot+RECT
[Shot]::GetClientRect($hwnd, [ref]$rect) | Out-Null
$origin = New-Object Shot+POINT
[Shot]::ClientToScreen($hwnd, [ref]$origin) | Out-Null
$width = $rect.R - $rect.L
$height = $rect.B - $rect.T
if ($width -lt 100 -or $height -lt 100) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    throw "captured window is only ${width}x${height}"
}

$bitmap = New-Object System.Drawing.Bitmap $width, $height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($origin.X, $origin.Y, 0, 0, $bitmap.Size)
$directory = Split-Path -Parent $Out
if ($directory -and -not (Test-Path $directory)) { New-Item -ItemType Directory -Force -Path $directory | Out-Null }
$bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()

[Shot]::SetWindowPos($hwnd, [IntPtr](-2), 0, 0, 0, 0, 0x0001 -bor 0x0040) | Out-Null
Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Write-Output "saved $Out ($width x $height)"
