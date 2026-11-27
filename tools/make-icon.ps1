# 从 assets/icon-source.png 生成：
#   assets/icon.ico    —— 多尺寸图标，供 build.rs 嵌入 exe 资源
#   assets/icon64.rgba —— 64x64 未预乘 RGBA 原始像素，供窗口/任务栏图标使用
# 源图是 1024x1024、白底、右下角带生成水印，这里裁掉并做四角透明。
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$assets = Join-Path $root 'assets'
$sourcePath = Join-Path $assets 'icon-source.png'
$icoPath = Join-Path $assets 'icon.ico'
$rgbaPath = Join-Path $assets 'icon64.rgba'

function Get-PixelBuffer([System.Drawing.Bitmap]$bmp) {
    $rect = New-Object System.Drawing.Rectangle 0, 0, $bmp.Width, $bmp.Height
    $data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $bytes = New-Object byte[] ($data.Stride * $bmp.Height)
    [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
    $stride = $data.Stride
    $bmp.UnlockBits($data)
    return @{ Bytes = $bytes; Stride = $stride }
}

function Set-PixelBuffer([System.Drawing.Bitmap]$bmp, [byte[]]$bytes) {
    $rect = New-Object System.Drawing.Rectangle 0, 0, $bmp.Width, $bmp.Height
    $data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::WriteOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    [System.Runtime.InteropServices.Marshal]::Copy($bytes, 0, $data.Scan0, $bytes.Length)
    $bmp.UnlockBits($data)
}

function New-Square([System.Drawing.Image]$src, [int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($src, 0, 0, $size, $size)
    $g.Dispose()
    return $bmp
}

function Get-PngBytes([System.Drawing.Bitmap]$bmp) {
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bytes = $ms.ToArray()
    $ms.Dispose()
    # 逗号避免 PowerShell 把字节数组展开成多个输出对象
    return , $bytes
}

if (-not (Test-Path $sourcePath)) { throw "缺少源图：$sourcePath" }

$source = [System.Drawing.Image]::FromFile($sourcePath)

# 原图 1024x1024：白底圆角方块约在 (92,86)-(940,940)，右下角 "AI 生成" 水印需要裁掉。
$cropRect = New-Object System.Drawing.Rectangle 92, 86, 848, 854
$cropped = New-Object System.Drawing.Bitmap 848, 854, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$cg = [System.Drawing.Graphics]::FromImage($cropped)
$cg.DrawImage($source, (New-Object System.Drawing.Rectangle 0, 0, 848, 854), $cropRect, [System.Drawing.GraphicsUnit]::Pixel)
$cg.Dispose()
$source.Dispose()

# 以 256 为母版：尺寸足够做 ico 最大帧，像素量又小。
$master = New-Square $cropped 256
$cropped.Dispose()

# 从四条边向内的连续白色 -> 透明，让圆角之外没有白底。
$buffer = Get-PixelBuffer $master
$bytes = $buffer.Bytes
$stride = $buffer.Stride
$size = 256

for ($y = 0; $y -lt $size; $y++) {
    $row = $y * $stride
    for ($x = 0; $x -lt $size; $x++) {
        $i = $row + $x * 4
        $minChannel = [Math]::Min([Math]::Min($bytes[$i], $bytes[$i + 1]), $bytes[$i + 2])
        if ($minChannel -lt 232) { break }
        $bytes[$i + 3] = [byte]([Math]::Max(0, [Math]::Min(255, [int]((255 - $minChannel) * 255 / 23))))
    }
    for ($x = $size - 1; $x -ge 0; $x--) {
        $i = $row + $x * 4
        $minChannel = [Math]::Min([Math]::Min($bytes[$i], $bytes[$i + 1]), $bytes[$i + 2])
        if ($minChannel -lt 232) { break }
        $bytes[$i + 3] = [byte]([Math]::Max(0, [Math]::Min(255, [int]((255 - $minChannel) * 255 / 23))))
    }
}

for ($x = 0; $x -lt $size; $x++) {
    for ($y = 0; $y -lt $size; $y++) {
        $i = $y * $stride + $x * 4
        $minChannel = [Math]::Min([Math]::Min($bytes[$i], $bytes[$i + 1]), $bytes[$i + 2])
        if ($minChannel -lt 232) { break }
        $bytes[$i + 3] = [byte]([Math]::Max(0, [Math]::Min(255, [int]((255 - $minChannel) * 255 / 23))))
    }
    for ($y = $size - 1; $y -ge 0; $y--) {
        $i = $y * $stride + $x * 4
        $minChannel = [Math]::Min([Math]::Min($bytes[$i], $bytes[$i + 1]), $bytes[$i + 2])
        if ($minChannel -lt 232) { break }
        $bytes[$i + 3] = [byte]([Math]::Max(0, [Math]::Min(255, [int]((255 - $minChannel) * 255 / 23))))
    }
}
Set-PixelBuffer $master $bytes

# 多尺寸 PNG 帧 -> ICO
$sizes = @(16, 24, 32, 48, 64, 128, 256)
$frames = New-Object 'System.Collections.Generic.List[byte[]]'
foreach ($s in $sizes) {
    $bmp = if ($s -eq 256) { $master } else { New-Square $master $s }
    $frames.Add([byte[]](Get-PngBytes $bmp))
    if ($s -ne 256) { $bmp.Dispose() }
}

$ico = New-Object System.IO.MemoryStream
$writer = New-Object System.IO.BinaryWriter $ico
$writer.Write([uint16]0)
$writer.Write([uint16]1)
$writer.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $frameBytes = $frames[$i]
    $dim = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $writer.Write([byte]$dim)
    $writer.Write([byte]$dim)
    $writer.Write([byte]0)
    $writer.Write([byte]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]32)
    $writer.Write([uint32]$frameBytes.Length)
    $writer.Write([uint32]$offset)
    $offset += $frameBytes.Length
}
foreach ($frame in $frames) { $writer.Write([byte[]]$frame) }
$writer.Flush()
[System.IO.File]::WriteAllBytes($icoPath, $ico.ToArray())
$writer.Dispose()
$ico.Dispose()

# 64x64 RGBA（egui IconData 要求未预乘、行优先）
$icon64 = New-Square $master 64
$rgbaInfo = Get-PixelBuffer $icon64
$rgba = New-Object byte[] (64 * 64 * 4)
for ($p = 0; $p -lt 64 * 64 * 4; $p += 4) {
    $rgba[$p] = $rgbaInfo.Bytes[$p + 2]
    $rgba[$p + 1] = $rgbaInfo.Bytes[$p + 1]
    $rgba[$p + 2] = $rgbaInfo.Bytes[$p]
    $rgba[$p + 3] = $rgbaInfo.Bytes[$p + 3]
}
[System.IO.File]::WriteAllBytes($rgbaPath, $rgba)
$icon64.Dispose()

# 预览图，便于人工确认裁切与透明效果
$previewPath = Join-Path $assets 'icon-preview.png'
$master.Save($previewPath, [System.Drawing.Imaging.ImageFormat]::Png)
$master.Dispose()

Get-ChildItem $icoPath, $rgbaPath, $previewPath | ForEach-Object { "{0,-20} {1,8:N0} bytes" -f $_.Name, $_.Length }

# 回读校验，确保 ico 结构有效
$checkIcon = New-Object System.Drawing.Icon($icoPath)
"icon valid: {0}x{1}" -f $checkIcon.Width, $checkIcon.Height
$checkIcon.Dispose()