# Derived Premiere sequence/export preset schema inventory

**Source scope:** 365 `.sqpreset`, 1,028 `.epr`, `Adobe Editing Modes.xml`, two private XMP definition files and `PrivateAudioFilterConfig.xml`.<br>
**Evidence level:** E2/E3 static schema evidence from one installation.

## Sequence presets

All 365 files have XML signatures and `SequencePreset` object graphs under `PremiereData Version="3"`.

| Property | Observed domain/count |
|---|---|
| `SequencePreset` version | v1: 48; v2: 9; v5: 288; v8: 20 |
| class ID | one observed GUID across all files |
| initial video tracks | 3 in all files |
| video rate | 23.976024, 24, 25, 29.97003, 30, 48, 50, 59.94006, 60 fps; one invalid token |
| audio rate | 32 kHz: 6; 48 kHz: 359 |
| field enum | 0: 325; 1: 28; 2: 12 |
| audio channel type | 1: 355; 3: 10 |
| audio-track record count | omitted: 57; 3: 20; 4: 280; 8: 8 |
| max bit depth | false in all observed files |
| max render quality | false in 317; absent in 48 |
| editing-mode GUIDs for Windows | 40 distinct |
| preview codec values for Windows | 26 distinct |

Largest/common frame-size counts include 1920x1080 (75), 1280x720 (50), 4096x2160 (38), 2048x1080 (35), 3840x2160 (34) and 1440x1080 (22). VR presets extend to 8192x8192 and 8192x4096.

Observed PAR rational pairs: `1/1` (305), `1920/1440` (30), `1920/1920` (10), `10/11` (6), `40/33` (6), `1024/702` (3), `768/702` (3), `3/2` (2).

### Audio-track JSON variants

- v5: object with `SerializerWrappedObject` array; records contain channel type, open/submix flags, matrix, name and pan.
- v8 VR/Ambisonics: direct array; records additionally contain sends, expanded height, panner assignments, solo, track ID and volume.
- v1/v2: `AudioTracks` may be absent.

These are versioned shapes. Empty/absent does not imply the same semantic state as an explicit empty array.

### Invalid sequence fixture

`DNxHD/720p 29.97/DNX SQ 720p 29.97.sqpreset` contains `VideoFrameRate=84SQ667200`, SHA-256 `41D825B30E740CFA66B26A93043A25F23DDE3DCF2B311FF9A9CF7ACBA810E2F0`, 6,251 bytes.

## Editing modes

`Adobe Editing Modes.xml` defines 52 modes. Observed rational frame-rate values include `10/1`, `12/1`, `15/1`, `23976/1000`, `24/1`, `24000/1001`, `25/1`, `25/2`, `30/1`, `30000/1001`, `48/1`, `50/1`, `60/1` and `60000/1001`. Observed PARs include `0/0` as a sentinel as well as square and anamorphic rational pairs.

Mode display names are localized. Platform-specific mode, player and recorder GUIDs are serialized separately and remain opaque.

## Export presets

1,027 of 1,028 `.epr` files parse as XML. Aggregate valid-corpus observations:

- 18 exporter class IDs;
- 34 exporter file-type IDs;
- 487 distinct `ParamIdentifier` strings;
- 56,617 `ExporterParam` records;
- parameter type codes 1-12;
- `DoVideo=true` in 999, false in 28;
- `DoAudio=true` in 926, false in 101.

Most frequent exporter class FourCC views are `MXF ` (414), `XDCA` (185), `????` (178), `NICK` (68), `P2MX` (44), `AME ` (40), `LORI` (36), `MPGI` (30) and `JEFF` (10). These are diagnostic renderings of unsigned 32-bit IDs, not display names.

Most frequent file-type FourCC views are `DMXF` (403), `MXFX` (185), `MooV` (119), `H264` (53), `MXF ` (44), `mpg2` (33), `MPEG` (30), `dvd ` (21), `mbd ` (20), `WMV ` (15), `AVIV` (13) and `HEVC` (10). Additional observed types include `WAVE`, `AIFF`, `MP3 `, `MP4 `, `AAC `, `DPX `, `PNG `, `TIFF`, `JPEG`, `DCP_` and GIF variants.

Every parameter record may carry type, value, arbitrary base64 data, child-container reference, target metadata and presentation/validation flags. Parameter order and container topology are part of the serialization.

### Invalid export fixture

`Settings/EncoderPresets/DVForDAW25.epr` contains an unescaped `<` in preset-comment text and is not well-formed XML. SHA-256 `94467A0E483D2F0B5B678404C237E6258C5106B5F92E69A45FC67C71E9CC6825`, 65,812 bytes.

## Private audio filter configuration

`PrivateAudioFilterConfig.xml` uses `prop.map version="4"` and declares two private VST3 descriptors: Loudness Radar and a DVA host checker. Descriptor fields include category, enabled, factory GUID, plugin ID, locator, mono/stereo/synth flags, vendor and version. Plugin/technology records describe type and search-location metadata.

This file does not enumerate the built-in DSP library and does not establish VST3 hosting parity by itself.

## Parser requirements

1. Prohibit DTD/entity resolution and bound XML/JSON/base64 resources.
2. Parse object identity and references before semantic mapping.
3. Preserve unknown tags, parameter IDs, enum values, FourCC/numeric IDs and original numeric text.
4. Version adapters for sequence-preset and audio-track shapes.
5. Treat malformed presets as recoverable diagnostics; never guess or rewrite the source during import.
6. Do not interpret localized name/description strings as stable identity.
