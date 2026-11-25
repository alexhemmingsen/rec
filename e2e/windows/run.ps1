# End-to-end test of Rec on Windows: drives the real app with SendInput and
# checks the overlay, Esc, the Stop pill, the clipboard path, the MP4, and
# that the pill is not in a full-screen recording (WDA_EXCLUDEFROMCAPTURE).
#
#   pwsh e2e/windows/run.ps1 -App target\release\Rec.exe
#
# Needs ffprobe and ffmpeg on PATH.
param([Parameter(Mandatory = $true)][string]$App)
$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class E2E {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT { public int dx, dy; public uint mouseData, dwFlags, time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Sequential)] struct KEYBDINPUT { public ushort wVk, wScan; public uint dwFlags, time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Explicit)] struct INPUTUNION { [FieldOffset(0)] public MOUSEINPUT mi; [FieldOffset(0)] public KEYBDINPUT ki; }
    [StructLayout(LayoutKind.Sequential)] struct INPUT { public uint type; public INPUTUNION u; }

    [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] inputs, int size);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] static extern bool SetProcessDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
    delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int max);
    [DllImport("user32.dll")] static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll")] static extern uint GetDpiForSystem();

    public static void DpiAware() { SetProcessDpiAwarenessContext(new IntPtr(-4)); }
    public static int ScreenW() { return GetSystemMetrics(0); }
    public static int ScreenH() { return GetSystemMetrics(1); }
    public static double Scale() { return GetDpiForSystem() / 96.0; }

    static INPUT Key(ushort vk, bool up) {
        var i = new INPUT { type = 1 };
        i.u.ki = new KEYBDINPUT { wVk = vk, dwFlags = up ? 2u : 0u };
        return i;
    }
    public static void Keys(params ushort[] vks) {
        var list = new List<INPUT>();
        foreach (var vk in vks) list.Add(Key(vk, false));
        for (int i = vks.Length - 1; i >= 0; i--) list.Add(Key(vks[i], true));
        foreach (var input in list) { SendInput(1, new[] { input }, Marshal.SizeOf(typeof(INPUT))); Thread.Sleep(30); }
    }
    static void Mouse(uint flags) {
        var i = new INPUT { type = 0 };
        i.u.mi = new MOUSEINPUT { dwFlags = flags };
        SendInput(1, new[] { i }, Marshal.SizeOf(typeof(INPUT)));
    }
    public static void Move(int x, int y) { SetCursorPos(x, y); Mouse(0x0001); }
    public static void Down() { Mouse(0x0002); }
    public static void Up() { Mouse(0x0004); }
    public static void Click(int x, int y) { Move(x, y); Thread.Sleep(150); Down(); Thread.Sleep(60); Up(); }
    public static void Drag(int x1, int y1, int x2, int y2) {
        Move(x1, y1); Thread.Sleep(300); Down(); Thread.Sleep(100);
        for (int i = 1; i <= 25; i++) { Move(x1 + (x2 - x1) * i / 25, y1 + (y2 - y1) * i / 25); Thread.Sleep(15); }
        Up();
    }

    /// Visible windows of a process: "left top width height title" lines.
    public static string[] Windows(uint pid) {
        var result = new List<string>();
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            if (p == pid && IsWindowVisible(h)) {
                RECT r; GetWindowRect(h, out r);
                var sb = new StringBuilder(256); GetWindowText(h, sb, 256);
                result.Add(r.Left + " " + r.Top + " " + (r.Right - r.Left) + " " + (r.Bottom - r.Top) + " " + sb.ToString());
            }
            return true;
        }, IntPtr.Zero);
        return result.ToArray();
    }
}
"@

[E2E]::DpiAware()
function Find-Tool($name) {
    $cmd = Get-Command $name -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $candidates = @("C:\ProgramData\chocolatey\bin\$name.exe") + (Get-ChildItem "C:\ProgramData\chocolatey\lib\ffmpeg*" -Recurse -Filter "$name.exe" -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })
    foreach ($c in $candidates) { if (Test-Path $c) { return $c } }
    throw "$name not found"
}
$ffprobe = Find-Tool "ffprobe"
$ffmpeg = Find-Tool "ffmpeg"
Write-Host "[e2e] ffprobe: $ffprobe"
$script:passed = 0
function Ok($what) { Write-Host "[e2e] ok: $what"; $script:passed++ }
function Fail($what) {
    Write-Host "[e2e] FAIL: $what"
    Write-Host "windows now:"; [E2E]::Windows($proc.Id) | ForEach-Object { Write-Host "  $_" }
    Stop-Process -Id $proc.Id -ErrorAction SilentlyContinue
    exit 1
}
function WaitFor($what, $timeout, [scriptblock]$cond) {
    $end = (Get-Date).AddSeconds($timeout)
    while ((Get-Date) -lt $end) { if (& $cond) { Ok $what; return }; Start-Sleep -Milliseconds 200 }
    Fail "timed out waiting for: $what"
}
function Wins { [E2E]::Windows($proc.Id) | ForEach-Object { $p = $_ -split ' ', 5; [pscustomobject]@{ X = [int]$p[0]; Y = [int]$p[1]; W = [int]$p[2]; H = [int]$p[3]; Title = $p[4] } } }
function Overlay { Wins | Where-Object { $_.Title -eq 'Rec selection' } | Select-Object -First 1 }
$scale = [E2E]::Scale()
# The Stop pill's window: the pill plus room for its shadow.
$pillW = [math]::Round(400 * $scale); $pillH = [math]::Round(76 * $scale)
function Pill { Wins | Where-Object { [math]::Abs($_.W - $pillW) -le 2 -and [math]::Abs($_.H - $pillH) -le 2 } | Select-Object -First 1 }
function Clip { try { (Get-Clipboard -Raw) } catch { "" } }
# Screenshots of each step for people to look at (REC_E2E_SHOTS = a folder).
function Snap($name) {
    if (-not $env:REC_E2E_SHOTS) { return }
    New-Item -ItemType Directory -Force $env:REC_E2E_SHOTS | Out-Null
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $img = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    [System.Drawing.Graphics]::FromImage($img).CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
    $img.Save((Join-Path $env:REC_E2E_SHOTS "$name.png"))
}
function Shortcut { [E2E]::Keys(0x11, 0x10, 0x35) } # Ctrl+Shift+5

$sw = [E2E]::ScreenW(); $sh = [E2E]::ScreenH()
Write-Host "[e2e] screen ${sw}x${sh}, scale $scale"
$data = Join-Path $env:APPDATA "com.alexhem.rec"
Remove-Item -Recurse -Force $data -ErrorAction SilentlyContinue

$proc = Start-Process -FilePath $App -PassThru
WaitFor "Rec's main window" 60 { (Wins | Where-Object { $_.W -ge 600 }).Count -gt 0 }
Start-Sleep 3

# 1 + 2: the shortcut shows the overlay; Esc cancels.
Shortcut
WaitFor "the selection overlay after the shortcut" 8 { $null -ne (Overlay) }
Start-Sleep -Milliseconds 300
Snap "01-after-shortcut"
$o = Overlay
if ($o.X -eq 0 -and $o.Y -eq 0 -and $o.W -eq $sw -and $o.H -eq $sh) { Ok "the overlay covers the screen" } else { Fail "overlay at $($o.X),$($o.Y) $($o.W)x$($o.H)" }
Start-Sleep -Milliseconds 500
[E2E]::Keys(0x1B)
WaitFor "Esc to cancel" 5 { $null -eq (Overlay) }

# 3: record a region (logical 400x300 at 160,140).
Set-Clipboard -Value " "
Shortcut
WaitFor "the overlay again" 8 { $null -ne (Overlay) }
Start-Sleep -Milliseconds 500
$x1 = [int](160 * $scale); $y1 = [int](140 * $scale); $x2 = [int](560 * $scale); $y2 = [int](440 * $scale)
[E2E]::Drag($x1, $y1, $x2, $y2)
WaitFor "the Stop pill" 10 { $null -ne (Pill) }
if ($null -ne (Overlay)) { Ok "the overlay stays up as a frame while recording" } else { Fail "the frame is missing" }
Start-Sleep -Milliseconds 300
Snap "02-recording"
$p = Pill
Write-Host "[e2e] pill at $($p.X),$($p.Y) $($p.W)x$($p.H); region $x1,$y1-$x2,$y2"
if ([math]::Abs($p.X + $p.W / 2 - $sw / 2) -le 3 -and [math]::Abs($p.Y - [math]::Round(28 * $scale)) -le 3) { Ok "the pill is at the top centre of the screen" } else { Fail "the pill isn't at the top centre" }
Start-Sleep 2.5
[E2E]::Click($p.X + [int]($p.W / 2), $p.Y + [int]($p.H / 2))
WaitFor "an .mp4 path on the clipboard" 15 { (Clip).Trim('"').EndsWith(".mp4") }
Snap "03-path-copied"
$file = (Clip).Trim().Trim('"')
if ($env:REC_E2E_SHOTS) { Copy-Item $file $env:REC_E2E_SHOTS -ErrorAction SilentlyContinue }
Write-Host "[e2e] clipboard: $file"
if ([System.IO.Path]::IsPathRooted($file)) { Ok "the path is absolute" } else { Fail "not absolute" }
if ((Split-Path $file -Leaf) -match '^rec-\d{4}-\d{2}-\d{2}-\d{2}-\d{2}-\d{2}(-\d+)?\.mp4$') { Ok "the file name matches rec-YYYY-MM-DD-HH-MM-SS.mp4" } else { Fail "bad name" }
if (Test-Path $file) { Ok "the file exists" } else { Fail "missing file" }
$info = & $ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height:format=duration -of default=nw=1 $file
Write-Host "[e2e] $($info -join ' ')"
if ($info -contains "codec_name=h264") { Ok "H.264" } else { Fail "not H.264" }
$ew = ($x2 - $x1) -band -2; $eh = ($y2 - $y1) -band -2
if ($info -contains "width=$ew" -and $info -contains "height=$eh") { Ok "the video has the region's size ($ew x $eh)" } else { Fail "wrong size, expected $ew x $eh" }
$dur = [double](($info | Where-Object { $_ -like "duration=*" }) -replace "duration=", "")
if ($dur -gt 1.8 -and $dur -lt 8) { Ok "the length matches the recording ($dur s)" } else { Fail "duration $dur" }
WaitFor "the pill to go away" 8 { $null -eq (Pill) }
WaitFor "the frame to go away" 5 { $null -eq (Overlay) }

# 4: a click without dragging doesn't cancel; only Esc does.
Set-Clipboard -Value " "
Shortcut
WaitFor "the overlay for the click test" 8 { $null -ne (Overlay) }
Start-Sleep -Milliseconds 500
[E2E]::Click([int]($sw / 2), [int]($sh / 2))
Start-Sleep 1
if ($null -ne (Overlay)) { Ok "a click doesn't cancel the selection" } else { Fail "a click cancelled the selection" }
[E2E]::Keys(0x1B)
WaitFor "Esc to cancel after the click" 5 { $null -eq (Overlay) }
Start-Sleep 1
if ($null -eq (Pill) -and (Clip).Trim() -eq "") { Ok "nothing was recorded" } else { Fail "something was recorded" }

# 5: full screen; the pill is inside the region but not in the video.
Shortcut
WaitFor "the overlay for the full-screen test" 8 { $null -ne (Overlay) }
Start-Sleep -Milliseconds 500
[E2E]::Drag(1, 1, $sw - 2, $sh - 2)
WaitFor "the Stop pill (full screen)" 10 { $null -ne (Pill) }
$p2 = Pill
Start-Sleep 2.5
[E2E]::Click($p2.X + [int]($p2.W / 2), $p2.Y + [int]($p2.H / 2))
WaitFor "a second .mp4 path" 15 { $c = (Clip).Trim().Trim('"'); $c.EndsWith(".mp4") -and $c -ne $file }
$file2 = (Clip).Trim().Trim('"')
$crop = "crop=$($p2.W):$($p2.H):$($p2.X - 1):$($p2.Y - 1)"
$raw = Join-Path $env:TEMP "rec-pill.rgb"
& $ffmpeg -v error -y -sseof -0.6 -i $file2 -frames:v 1 -vf $crop -f rawvideo -pix_fmt rgb24 $raw
$bytes = [System.IO.File]::ReadAllBytes($raw)
$red = 0; $total = $bytes.Length / 3
for ($i = 0; $i -lt $bytes.Length; $i += 3) { if ($bytes[$i] -gt 200 -and $bytes[$i + 1] -lt 110 -and $bytes[$i + 2] -lt 100) { $red++ } }
Write-Host "[e2e] pill area in the video: $total px, $red stop-button red"
if ($total -gt 0 -and $red -lt [math]::Max(4, $total / 200)) { Ok "the Stop pill is not in the video" } else { Fail "the pill shows up in the video" }

# 6: History lists both recordings.
$history = Get-Content -Raw (Join-Path $data "history.json")
if ($history.Contains(($file -replace '\\', '\\')) -and $history.Contains(($file2 -replace '\\', '\\'))) { Ok "History lists both recordings" } else { Fail "not in History" }

Stop-Process -Id $proc.Id -ErrorAction SilentlyContinue
Write-Host "[e2e] PASS ($script:passed checks)"
