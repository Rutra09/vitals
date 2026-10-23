# Renders the Vitals mark into a multi-size Windows icon and a documentation PNG.
# Usage: powershell -ExecutionPolicy Bypass -File tools/make-icon.ps1

Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$assets = Join-Path $root 'crates\vitals-app\assets'
New-Item -ItemType Directory -Force -Path $assets | Out-Null

$sizes = @(16, 20, 24, 32, 40, 48, 64, 128, 256)
$payloads = @()
$report = @()

function New-Mark([int]$size) {
    $scale = 8
    $px = $size * $scale
    $big = New-Object System.Drawing.Bitmap $px, $px
    $g = [System.Drawing.Graphics]::FromImage($big)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)

    $rect = New-Object System.Drawing.RectangleF 0, 0, $px, $px
    $radius = [float]($px * 0.29)
    $d = $radius * 2
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddArc(0, 0, $d, $d, 180, 90)
    $path.AddArc($px - $d, 0, $d, $d, 270, 90)
    $path.AddArc($px - $d, $px - $d, $d, $d, 0, 90)
    $path.AddArc(0, $px - $d, $d, $d, 90, 90)
    $path.CloseFigure()

    $start = [System.Drawing.Color]::FromArgb(255, 10, 132, 255)
    $end = [System.Drawing.Color]::FromArgb(255, 123, 92, 246)
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush $rect, $start, $end, 45.0
    $g.FillPath($brush, $path)

    $unit = $px / 28.0
    $points = New-Object 'System.Drawing.PointF[]' 6
    $points[0] = New-Object System.Drawing.PointF ([float]($px / 2.0 - 8.4 * $unit)), ([float]($px / 2.0 + 0.4 * $unit))
    $points[1] = New-Object System.Drawing.PointF ([float]($px / 2.0 - 3.6 * $unit)), ([float]($px / 2.0 + 0.4 * $unit))
    $points[2] = New-Object System.Drawing.PointF ([float]($px / 2.0 - 1.7 * $unit)), ([float]($px / 2.0 - 5.2 * $unit))
    $points[3] = New-Object System.Drawing.PointF ([float]($px / 2.0 + 1.5 * $unit)), ([float]($px / 2.0 + 5.7 * $unit))
    $points[4] = New-Object System.Drawing.PointF ([float]($px / 2.0 + 3.6 * $unit)), ([float]($px / 2.0 + 0.4 * $unit))
    $points[5] = New-Object System.Drawing.PointF ([float]($px / 2.0 + 8.4 * $unit)), ([float]($px / 2.0 + 0.4 * $unit))

    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), ([float](1.85 * $unit))
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
    $g.DrawLines($pen, $points)
    $pen.Dispose()
    $brush.Dispose()
    $path.Dispose()
    $g.Dispose()

    $final = New-Object System.Drawing.Bitmap $size, $size
    $fg = [System.Drawing.Graphics]::FromImage($final)
    $fg.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $fg.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $fg.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $fg.DrawImage($big, 0, 0, $size, $size)
    $fg.Dispose()
    $big.Dispose()
    return $final
}

function Read-Pixels($bitmap) {
    $rect = New-Object System.Drawing.Rectangle 0, 0, $bitmap.Width, $bitmap.Height
    $locked = $bitmap.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $bytes = New-Object byte[] ($locked.Stride * $bitmap.Height)
    [System.Runtime.InteropServices.Marshal]::Copy($locked.Scan0, $bytes, 0, $bytes.Length)
    $stride = $locked.Stride
    $bitmap.UnlockBits($locked)
    return @{ Bytes = $bytes; Stride = $stride }
}

function ConvertTo-BmpIcon($bitmap) {
    $pixels = Read-Pixels $bitmap
    $w = $bitmap.Width
    $h = $bitmap.Height
    $row = [math]::Floor(($w + 31) / 32) * 4
    $sizeImage = ($w * 4 + $row) * $h
    $ms = New-Object System.IO.MemoryStream
    $bw = New-Object System.IO.BinaryWriter $ms
    $bw.Write([uint32]40)
    $bw.Write([int32]$w)
    $bw.Write([int32]($h * 2))
    $bw.Write([uint16]1)
    $bw.Write([uint16]32)
    $bw.Write([uint32]0)
    $bw.Write([uint32]$sizeImage)
    $bw.Write([int32]0)
    $bw.Write([int32]0)
    $bw.Write([uint32]0)
    $bw.Write([uint32]0)
    for ($y = $h - 1; $y -ge 0; $y--) {
        $offset = $y * $pixels.Stride
        for ($x = 0; $x -lt $w; $x++) {
            $i = $offset + $x * 4
            $bw.Write($pixels.Bytes[$i])
            $bw.Write($pixels.Bytes[$i + 1])
            $bw.Write($pixels.Bytes[$i + 2])
            $bw.Write($pixels.Bytes[$i + 3])
        }
    }
    for ($y = $h - 1; $y -ge 0; $y--) {
        $offset = $y * $pixels.Stride
        for ($byte = 0; $byte -lt $row; $byte++) {
            $value = 0
            for ($bit = 0; $bit -lt 8; $bit++) {
                $x = $byte * 8 + $bit
                if ($x -lt $w) {
                    $i = $offset + $x * 4
                    if ($pixels.Bytes[$i + 3] -eq 0) { $value = $value -bor (1 -shl (7 - $bit)) }
                }
            }
            $bw.Write([byte]$value)
        }
    }
    $bw.Flush()
    $result = $ms.ToArray()
    $bw.Dispose()
    $ms.Dispose()
    return $result
}

foreach ($size in $sizes) {
    $bitmap = New-Mark $size
    $ms = New-Object System.IO.MemoryStream
    $bitmap.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $payload = $ms.ToArray()
    $ms.Dispose()
    $payloads += ,$payload
    $report += [pscustomobject]@{ Size = $size; Bytes = $payload.Length }
    if ($size -eq 256) {
        $bitmap.Save((Join-Path $assets 'icon.png'), [System.Drawing.Imaging.ImageFormat]::Png)
    }
    $bitmap.Dispose()
}

$icoPath = Join-Path $assets 'vitals.ico'
$stream = New-Object System.IO.MemoryStream
$bw = New-Object System.IO.BinaryWriter $stream
$bw.Write([uint16]0)
$bw.Write([uint16]1)
$bw.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $size = $sizes[$i]
    $dim = if ($size -ge 256) { 0 } else { $size }
    $bw.Write([byte]$dim)
    $bw.Write([byte]$dim)
    $bw.Write([byte]0)
    $bw.Write([byte]0)
    $bw.Write([uint16]1)
    $bw.Write([uint16]32)
    $bw.Write([uint32]$payloads[$i].Length)
    $bw.Write([uint32]$offset)
    $offset += $payloads[$i].Length
}
foreach ($payload in $payloads) { $bw.Write($payload) }
$bw.Flush()
[System.IO.File]::WriteAllBytes($icoPath, $stream.ToArray())
$bw.Dispose()
$stream.Dispose()

$report | Format-Table -AutoSize
Write-Output "Wrote $icoPath"
