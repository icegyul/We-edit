# Layouts de parámetros de efectos (MatchName -> parámetros)

> Extraído de los presets de fábrica. El índice es la posición del parámetro dentro del
> componente en un `.prproj`, necesario para leer/escribir proyectos de Premiere.

Tipos `ParameterControlType` observados: 1 entero · 2 float · 3 ángulo · 4 checkbox · 5 color (64 bits) ·
6 punto (x:y normalizado) · 7 popup · 8 float (slider) · 9/10 datos binarios base64 · 11 inicio de grupo ·
12 fin de grupo · 16 oculto.

## `AE.ADBE Bevel Edges` - Bevel Edges

| # | Clase | Nombre | Tipo | Default | Mín | Máx |
|---|---|---|---|---|---|---|
| 1 | Video | Edge Thickness | 2 | 0.150000005960 | 0.000000000000 | 0.500000000000 |
| 2 | Video | Light Angle | 3 | -45.000000000000 | -32768.000000000000 | 32767.000000000000 |
| 3 | Video | Light Color | 5 | 18374966859414961920 | 0 | 18446744073709551615 |
| 4 | Video | Light Intensity | 2 | 0.300000011921 | 0.000000000000 | 1.000000000000 |

## `AE.ADBE Fast Blur` - Fast Blur

| # | Clase | Nombre | Tipo | Default | Mín | Máx |
|---|---|---|---|---|---|---|
| 1 | Video | Blurriness | 2 | 0.000000000000 | 0.000000000000 | 32767.000000000000 |
| 2 | Video | Blur Dimensions | 7 | 0 | 0 | 2 |
| 3 | Video |  | 4 | false | false | true |

## `AE.ADBE Graphic SubGroup` - Group

| # | Clase | Nombre | Tipo | Default | Mín | Máx |
|---|---|---|---|---|---|---|
| 1 | Point | Position | 6 | 0.5:0.5 | None | None |
| 2 | Video | Scale | 2 | 100. | 0 | 10000 |
| 3 | Video | Scale Width | 2 | 100. | 0 | 10000 |
| 4 | Video |  | 4 | true | false | true |
| 5 | Video | Rotation | 3 | 0. | -32768 | 32767 |
| 6 | Point | Anchor Point | 6 | 0.5:0.5 | None | None |

## `AE.ADBE Lumetri` - Lumetri Color

| # | Clase | Nombre | Tipo | Default | Mín | Máx |
|---|---|---|---|---|---|---|
| 1 | ArbVideo | Blob | 10 |  | None | None |
| 2 | Video |  | 4 | false | false | true |
| 3 | Video | Basic Correction | 11 | false | false | false |
| 4 | Video |  | 4 | true | false | true |
| 5 | ArbVideo |  | 10 |  | None | None |
| 6 | ArbVideo |  | 10 |  | None | None |
| 7 | Video | Input LUT | 7 | 0 | 0 | 998 |
| 8 | Video | HDR White | 8 | 100. | 100. | 1000. |
| 9 | Video | White Balance | 11 | false | false | false |
| 10 | Video | WB Selector | 5 | 18374897589125431296 | 0 | 18446744073709551615 |
| 11 | Video | Temperature | 8 | 0. | -150. | 150. |
| 12 | Video | Tint | 8 | 0. | -150. | 150. |
| 13 | Video |  | 12 | false | false | false |
| 14 | Video | Tone | 11 | false | false | false |
| 15 | Video | Exposure | 8 | 0. | -7. | 7. |
| 16 | Video | Contrast | 8 | 0. | -150. | 150. |
| 17 | Video | Highlights | 8 | 0. | -150. | 150. |
| 18 | Video | Shadows | 8 | 0. | -150. | 150. |
| 19 | Video | Whites | 8 | 0. | -150. | 150. |
| 20 | Video | Blacks | 8 | 0. | -150. | 150. |
| 21 | Video | HDR Specular | 8 | 0. | -150. | 150. |
| 22 | Video |  | 16 | false | false | true |
| 23 | Video |  | 16 | false | false | true |
| 24 | Video |  | 12 | false | false | false |
| 25 | Video | Saturation | 8 | 100. | 0. | 300. |
| 26 | Video |  | 12 | false | false | false |
| 27 | Video | Creative | 11 | false | false | false |
| 28 | Video |  | 4 | true | false | true |
| 29 | ArbVideo |  | 10 |  | None | None |
| 30 | ArbVideo |  | 10 |  | None | None |
| 31 | Video | Look | 7 | 1 | 0 | 998 |
| 32 | Video | Intensity | 8 | 100. | 0. | 200. |
| 33 | Video | Adjustments | 11 | false | false | false |
| 34 | Video | Faded Film | 8 | 0. | 0. | 150. |
| 35 | Video | Sharpen | 8 | 0. | -100. | 100. |
| 36 | Video | Vibrance | 8 | 0. | -100. | 100. |
| 37 | Video | Saturation | 8 | 100. | 0. | 300. |
| 38 | ArbVideo |  | 9 |  | None | None |
| 39 | Video | Tint Balance | 8 | 0. | -150. | 150. |
| 40 | Video |  | 12 | false | false | false |
| 41 | Video |  | 12 | false | false | false |
| 42 | Video | Curves | 11 | false | false | false |
| 43 | Video |  | 4 | true | false | true |
| 44 | Video | RGB Curves | 11 | false | false | false |
| 45 | Video | HDR Range | 8 | 100. | 100. | 10000. |
| 46 | ArbVideo |  | 9 |  | None | None |
| 47 | Video |  | 12 | false | false | false |
| 48 | Video | Hue Saturation Curve | 11 | false | false | false |
| 49 | ArbVideo |  | 9 |  | None | None |
| 50 | Video |  | 12 | false | false | false |
| 51 | Video |  | 12 | false | false | false |
| 52 | Video | Color Wheels | 11 | false | false | false |
| 53 | Video |  | 4 | true | false | true |
| 54 | Video | HDR White | 8 | 100. | 100. | 1000. |
| 55 | ArbVideo |  | 9 |  | None | None |
| 56 | Video |  | 12 | false | false | false |
| 57 | Video | HSL Secondary | 11 | false | false | false |
| 58 | Video |  | 4 | true | false | true |
| 59 | Video | Key | 11 | false | false | false |
| 60 | Video | Set color | 5 | 18374686479671623680 | 0 | 18446744073709551615 |
| 61 | Video | Add color | 5 | 18374686479671623680 | 0 | 18446744073709551615 |
| 62 | Video | Remove color | 5 | 18374686479671623680 | 0 | 18446744073709551615 |
| 63 | ArbVideo |  | 9 |  | None | None |
| 64 | Video |  | 4 | false | false | true |
| 65 | Video |  | 7 | 0 | 0 | 2 |
| 66 | Video |  | 4 | false | false | true |
| 67 | Video |  | 16 | false | false | true |
| 68 | Video |  | 16 | false | false | true |
| 69 | Video |  | 12 | false | false | false |
| 70 | Video | Refine | 11 | false | false | false |
| 71 | Video | Denoise | 8 | 0. | 0. | 100. |
| 72 | Video | Blur | 8 | 0. | 0. | 1000. |
| 73 | Video | Blur | 12 | false | false | false |
| 74 | Video | Correction | 11 | false | false | false |
| 75 | ArbVideo |  | 9 |  | None | None |
| 76 | ArbVideo |  | 10 |  | None | None |
| 77 | Video | Temperature | 8 | 0. | -300. | 300. |
| 78 | Video | Tint | 8 | 0. | -300. | 300. |
| 79 | Video | Contrast | 8 | 0. | -150. | 150. |
| 80 | Video | Sharpen | 8 | 0. | -100. | 100. |
| 81 | Video | Saturation | 8 | 100. | 0. | 300. |
| 82 | Video | Saturation | 12 | false | false | false |
| 83 | Video |  | 12 | false | false | false |
| 84 | Video | Vignette | 11 | false | false | false |
| 85 | Video |  | 4 | true | false | true |
| 86 | Video | Amount | 8 | 0. | -5. | 5. |
| 87 | Video | Midpoint | 8 | 50. | 0. | 100. |
| 88 | Video | Roundness | 8 | 0. | -100. | 100. |
| 89 | Video | Feather | 8 | 50. | 0. | 100. |
| 90 | Video |  | 12 | false | false | false |
| 91 | Video | SpeedGrade Custom | 11 | false | false | false |
| 92 | Video | Custom Layer | 4 | true | false | true |
| 93 | Video | unused | 16 | false | false | true |
| 94 | Video | unused | 16 | false | false | true |
| 95 | ArbVideo |  | 10 |  | None | None |
| 96 | Video |  | 12 | false | false | false |
| 97 | Video |  | 4 | true | false | true |
| 98 | ArbVideo | Embedded LUTs | 10 |  | None | None |

## `AE.ADBE Mosaic` - Mosaic

| # | Clase | Nombre | Tipo | Default | Mín | Máx |
|---|---|---|---|---|---|---|
| 1 | Video | Horizontal Blocks | 1 | 1 | 1 | 4000 |
| 2 | Video | Vertical Blocks | 1 | 1 | 1 | 4000 |
| 3 | Video |  | 4 | true | false | true |

## `AE.ADBE Motion` - Motion

| # | Clase | Nombre | Tipo | Default | Mín | Máx |
|---|---|---|---|---|---|---|
| 1 | Point | Position | 6 | 0.274305000000000020000000:0.75208299999 | None | None |
| 2 | Video | Scale | 2 | 25.000000000000 | 0.000000000000 | 600.000000000000 |
| 3 | Video | Scale Width | 2 | 100.000000000000 | 0.000000000000 | 600.000000000000 |
| 4 | Video |  | 4 | true | false | true |
| 5 | Video | Rotation | 3 | 0.000000000000 | -32768.000000000000 | 32767.000000000000 |
| 6 | Point | Anchor Point | 6 | 0.500000000000000000000000:0.50000000000 | None | None |

## `AE.ADBE Shape` - Shape

| # | Clase | Nombre | Tipo | Default | Mín | Máx |
|---|---|---|---|---|---|---|
| 1 | ArbVideo | Path | 22 |  | None | None |
| 2 | ArbVideo | Appearance | 9 |  | None | None |
| 3 | Video | Transform | 11 | false | false | false |
| 4 | Point | Position | 6 | 0.18564815074205399:0.26354166865348816 | None | None |
| 5 | Video | Scale | 2 | 100. | 0 | 4000 |
| 6 | Video | Horizontal Scale | 2 | 100. | 0 | 4000 |
| 7 | Video |  | 4 | true | false | true |
| 8 | Video | Rotation | 3 | 0. | -32768 | 32767 |
| 9 | Video | Opacity | 2 | 100. | 0 | 100 |
| 10 | Point | Anchor Point | 6 | 0.1388888888888889:0.052083333333333336 | None | None |
| 11 | Video |  | 12 | false | false | false |
| 12 | Video |  | 4 | false | false | true |
| 13 | Video |  | 4 | false | false | true |
| 14 | Video |  | 4 | false | false | true |
| 15 | Video |  | 4 | false | false | true |
| 16 | Video | Parent Width | 2 | 0. | 0 | 20000 |
| 17 | Video | Parent Height | 2 | 0. | 0 | 20000 |
| 18 | Video | Parent Rotation | 3 | 0. | -32768 | 32767 |

## `AE.ADBE Text` - Text

| # | Clase | Nombre | Tipo | Default | Mín | Máx |
|---|---|---|---|---|---|---|
| 1 | ArbVideo | Source Text | 9 |  | None | None |
| 2 | Video | Transform | 11 | false | false | false |
| 3 | Point | Position | 6 | 0.5:0.5 | None | None |
| 4 | Video | Scale | 2 | 100. | 0 | 4000 |
| 5 | Video | Horizontal Scale | 2 | 100. | 0 | 4000 |
| 6 | Video |  | 4 | true | false | true |
| 7 | Video | Rotation | 3 | 0. | -32768 | 32767 |
| 8 | Video | Opacity | 2 | 100. | 0 | 100 |
| 9 | Point | Anchor Point | 6 | 0:0 | None | None |
| 10 | Video |  | 12 | false | false | false |
| 11 | Video |  | 8 | 0. | 0 | 32768 |
| 12 | Video |  | 8 | 0. | 0 | 32768 |
| 13 | Video | start | 8 | 1. | -100 | 1000000000 |
| 14 | Video | end | 8 | 1. | -100 | 1000000000 |
| 15 | Video |  | 4 | false | false | true |
| 16 | Video |  | 4 | false | false | true |
| 17 | Video |  | 4 | false | false | true |
| 18 | Video |  | 4 | false | false | true |
| 19 | Video | Parent Width | 2 | 0. | 0 | 20000 |
| 20 | Video | Parent Height | 2 | 0. | 0 | 20000 |
| 21 | Video | Parent Rotation | 3 | 0. | -32768 | 32767 |
| 22 | Video |  | 4 | false | false | true |

## `AE.ADBE Twirl` - Twirl

| # | Clase | Nombre | Tipo | Default | Mín | Máx |
|---|---|---|---|---|---|---|
| 1 | Video | Angle | 3 | 0.000000000000 | -32768.000000000000 | 32767.000000000000 |
| 2 | Video | Twirl Radius | 2 | 75.000000000000 | 0.000000000000 | 100.000000000000 |
| 3 | Point | Twirl Center | 6 | 0.500000000000000000000000:0.50000000000 | None | None |

## `PR.ADBE Convolution Kernel New` - Convolution Kernel

| # | Clase | Nombre | Tipo | Default | Mín | Máx |
|---|---|---|---|---|---|---|
| 1 | Video | M11 | 1 | 0 | -30 | 30 |
| 2 | Video | M12 | 1 | 1 | -30 | 30 |
| 3 | Video | M13 | 1 | 0 | -30 | 30 |
| 4 | Video | M21 | 1 | 1 | -30 | 30 |
| 5 | Video | M22 | 1 | 2 | -30 | 30 |
| 6 | Video | M23 | 1 | 1 | -30 | 30 |
| 7 | Video | M31 | 1 | 0 | -30 | 30 |
| 8 | Video | M32 | 1 | 1 | -30 | 30 |
| 9 | Video | M33 | 1 | 0 | -30 | 30 |
| 10 | Video | Offset | 1 | 0 | -32768 | 32767 |
| 11 | Video | Scale | 1 | 6 | -32768 | 32767 |
| 12 | Video | Process Alpha | 4 | true | false | true |

## `PR.ADBE Solarize` - Solarize

| # | Clase | Nombre | Tipo | Default | Mín | Máx |
|---|---|---|---|---|---|---|
| 1 | Video | Threshold | 1 | 254 | 0 | 254 |
