# Evidence manifest

This manifest identifies private local evidence without redistributing it. Paths are workstation-local and are not requirements for contributors. SHA-256 is the stable identity.

## Premiere Pro 2024 project data

| Evidence ID | Local source | Bytes | SHA-256 | Use |
|---|---|---:|---|---|
| EV-PR-001 | `TemplateProjects/Broadcaster Template Project.prproj` | 36,026 | `C789163BE2F46A73BDA9BE3C1A3E05B2D37749C632E62A52AF6721BA250EC2E1` | Container, classes, sequences |
| EV-PR-002 | `TemplateProjects/Social Media Template Project.prproj` | 260,744 | `4455E563F0BA3D0F54FD6DEF4FF53E32CD797EC6CDC1E7608EAB99B5E5F4643C` | Tracks, clips, graphics, effects |
| EV-PR-003 | `TemplateProjects/Standard Template Project.prproj` | 16,997 | `3387182DEB888FBC52FC0B9B45BBB733BC888691DD85BD3D240B59B556714223` | Defaults, classes, sequence settings |
| EV-PR-004 | User-created empty project `Sin título.prproj` | 6,329 | `DE8E56CD137E396560F045D4D23D92A1D884B7BB9B0E400E551FCAA7DDD54EFB` | Independent container/root check |
| EV-PR-005 | User autosave `Adobe Premiere Pro Auto-Save/Sin título--…--2026-09-28_00-35-20.prproj` | 12,299 | `37717FAB6C342841AEFA94334868F150D8906589AB4463840216EAFB17799620` | Independent one-sequence graph; automation/default track fields |

All five files begin with gzip magic `1F 8B`. The three factory templates have gzip OS byte `0x13`. EV-PR-004 decompresses to 71,567 UTF-8 bytes and has root `<PremiereData Version="3">`; it contains no sequence and therefore cannot validate timeline serialization. EV-PR-005 decompresses to 110,078 UTF-8 bytes and contains one direct `Sequence` definition.

## Shortcut and preset data

| Evidence ID | Local source | Bytes | SHA-256 | Use |
|---|---|---:|---|---|
| EV-KBD-001 | `Keyboard Shortcuts/en/Adobe Premiere Pro Defaults.kys` | 99,955 | `B12217E4B11E5B7AD9234ECB2E754177AC5892E548ABFCAADFC1DFF55D7AA519` | 370 default Windows bindings |
| EV-KBD-002 | `ksvlayout/xml/es.ksvlayout` | 20,234 | `889AC9D30A84ED1D51505CB9D01C4DBFF9035F83CF213CD7D691BC5A11C2C0B7` | Representative physical-layout fragment and key-code domains |
| EV-FX-001 | `LocalizedPresets/en_US/Effect Presets/Factory Presets.prfpset` | 475,546 | `0208C63B74B7966D7A814D84DA83D081D8B73CCDA29592862A0EE25615F0250F` | Match names, parameter classes, keyframe layout |
| EV-FX-002 | `LocalizedPresets/en_US/Effect Presets/Lumetri Presets.prfpset` | 24,343,073 | `2BDBA2391B91FC157892474541C01FCE00BA0F78E324CDA77B5EF05865A030B9` | Lumetri parameter layout and ranges |
| EV-FX-003 | `Settings/MaskPresets.prfpset` | 20,097 | `B45CAFA8E1A53176E47D392136C7E4BE59E0A0F872DB209699097059F19AAE8C` | Mask parameter order and two path payload fixtures |

## Preset, metadata, and audio configuration data

| Evidence ID | Local source | Bytes | SHA-256 | Use |
|---|---|---:|---|---|
| EV-SEQ-001 | `Settings/Editing Modes/Adobe Editing Modes.xml` | 231,831 | `5C02BDF78BB86165ACF5B8A310EAA0E0A5743DC39CCB79123C84A9F8D4E8626D` | 52 editing modes, rational rates/PARs, platform GUID maps |
| EV-SEQ-002 | `Settings/SequencePresets/DNxHD/720p 29.97/DNX SQ 720p 29.97.sqpreset` | 6,251 | `41D825B30E740CFA66B26A93043A25F23DDE3DCF2B311FF9A9CF7ACBA810E2F0` | Malformed/nonnumeric frame-duration fixture |
| EV-EXP-001 | `Settings/EncoderPresets/DVForDAW25.epr` | 65,812 | `94467A0E483D2F0B5B678404C237E6258C5106B5F92E69A45FC67C71E9CC6825` | Malformed XML export-preset fixture |
| EV-META-001 | `Settings/premiere_private_project_definitions.xml` | 17,662 | `CE252206749D25CE609AEF0F5912FD98BF602500B6C124EF4BCAAD85A8C1D392` | 63 project metadata definitions and timecode qualifiers |
| EV-META-002 | `Settings/premiere_private_file_properties_definitions.xml` | 1,285 | `A40639E7A1E8FB274BD34663A32C75E51631D58AC465C512E166906175F3C157` | 7 file-property definitions |
| EV-AUD-001 | `Settings/PrivateAudioFilterConfig.xml` | 5,640 | `08F1067E9D8F2A175C93AF69E08BD3E34CA2F62DF21F11085C1ABAD4E86A6924` | Private VST3 descriptor/configuration shape |

## Workspace-layout data

| Evidence ID | Local source | Bytes | SHA-256 |
|---|---|---:|---|
| EV-WS-001 | `XML/ALLPANELSWORKSPACELAYOUT.xml` | 418,575 | `53626DDFA61CB3C334C458C6BC68AB92ED53E206767A72B55831E0A42DC8D050` |
| EV-WS-002 | `XML/ASSEMBLYWORKSPACELAYOUT.xml` | 483,825 | `56E00847F61C995CB4989729FBFE66B9C25EAFD508350AD28F10EC2EF213DC2F` |
| EV-WS-003 | `XML/AUDIOWORKSPACELAYOUT.xml` | 318,720 | `87B00D9D8C04DA3613B3A84A7FA5C8422A1A9A3FB236F6C3A4142AEF79737569` |
| EV-WS-004 | `XML/CAPTIONSWORKSPACELAYOUT.xml` | 475,917 | `62F009B076A96D2F41B3743EC05450F98190501EF7D1FE27F8C6F12A75F809F3` |
| EV-WS-005 | `XML/COLORWORKSPACELAYOUT.xml` | 449,342 | `C0B3CDA06D9F31D370907871944FD79A46D669CA182DB6DC9C56A62C9174B1B4` |
| EV-WS-006 | `XML/EDITINGWORKSPACELAYOUT.xml` | 511,965 | `4F95B14F667A1ECE3F25F80A92DC21870657678EF2C85565638F7C97B367F222` |
| EV-WS-007 | `XML/EFFECTSWORKSPACELAYOUT.xml` | 522,306 | `987A96D99132E35CE81C699B8A40F4C7E993813E69E0C6082329349308C35CF5` |
| EV-WS-008 | `XML/ESSENTIALSWORKSPACELAYOUT.xml` | 712,120 | `AD5520CFF72A1E9EC0918B9E9783BE2A53D83424D177CF3982D196B9C1D31D5E` |
| EV-WS-009 | `XML/LEARNINGWORKSPACELAYOUT.xml` | 495,359 | `64ECDA41075E3B28D533C4041583FACB59C2A0A1663862B9DE1DBA9E25953CE1` |
| EV-WS-010 | `XML/LIBRARIESWORKSPACELAYOUT.xml` | 481,446 | `B0D2EF11F5AE0AE5B012CFB60C3543B63D72AFD884E3E7D5FB4477780A1E2ED7` |
| EV-WS-011 | `XML/METALOGGINGWORKSPACELAYOUT.xml` | 290,849 | `83901F418A6BE7883BE1B6BDDC65F5F9748C84F64650CD737738976690DA3EE8` |
| EV-WS-012 | `XML/PRODUCTIONWORKSPACELAYOUT.xml` | 512,103 | `5FEC8CDD13E226D167BC454B5D797AC8382D45C301DE884D2D1FC14F02DE758F` |
| EV-WS-013 | `XML/REVIEWWORKSPACELAYOUT.xml` | 527,299 | `901E9F719D65EF18BBA244529B8B74BB2C66E0EA7922F1446BBE8A8B65D75F36` |
| EV-WS-014 | `XML/SOCIALWORKSPACELAYOUT.xml` | 461,475 | `0C3F6FFBB5FD7974E859E839FF1F8371BEA1C2B50831A32928AF2CF53B0130AF` |
| EV-WS-015 | `XML/TEXTBASEDEDITINGWORKSPACELAYOUT.xml` | 603,286 | `26F99394840B170F3D4588B4A810D40A175CF82E6CE78B064E23F2E17DEE7A8B` |
| EV-WS-016 | `XML/VERTICALWORKSPACELAYOUT.xml` | 546,039 | `C3E3508CF8B54E8EA935B421B3296A535B8FA5862B7F17439128AFD7DF05B5C5` |

## Corpus fingerprints

Each corpus fingerprint hashes UTF-8 without BOM over LF-terminated lines of `relative/path<TAB>bytes<TAB>lowercase-file-sha256`, sorted by ordinal relative path. It proves the exact audited file set without committing the source files.

| Corpus | Files | Bytes | Manifest SHA-256 |
|---|---:|---:|---|
| Sequence presets | 365 | 2,299,998 | `DCE64B41C059D5BC52CCD1D3864733D7CD987A1FC10438713700B8EFDB6A34F0` |
| Export presets | 1,028 | 64,365,290 | `3FE62F4DB062BA3B5F18B29429ABF7B08556186E28EFE8E888B1CB75DA2EDA5B` |
| Workspace layouts | 16 | 7,810,626 | `0917FA7C2D203F497C200B25FAE407D386D181B1C2F6D648CD16DD2A9B40FD12` |
| Effect presets | 21 | 249,036,118 | `FA48DB21B9C6BABB935FC0D8C64FEAC7C23A098A10512D36B8F281793B97BCA0` |
| UXP manifests, including one nested Photoshop-only manifest | 11 | 16,763 | `64AAE46622DF5FC9BAA0D599D4D9605D938723426BE1BC969681D7932CD606E9` |
| All shortcut maps | 41 | 3,045,521 | `534D20AF777349432DFB3BED58E3A468E62366BE56325DEA57A458D7EE3DF961` |
| Localized Premiere Defaults maps | 11 | 988,031 | `56176D9D1952A3B7205659501A20A142CF1F2C73842E6C4C5077212DAC95BA9E` |
| Physical keyboard-layout fragments | 10 | 201,837 | `21CB94F8BD830C988351C7B72778DAAD2D01A3C04D823C694F7B4FC4CD7CE278` |

## Committed derived indexes

| File | SHA-256 |
|---|---|
| `premiere_shortcuts_default.json` | `7DC5CFCAB0D8FE7EB244ECB23CC12287BB8D865B291DB9E8943082333CFEB3DE` |
| `premiere_shortcuts_default.md` | `C22EE4D9D92C46AC2D18AD65ACD4D01B110880878C4F70E62E3C942EA0B53D9A` |
| `premiere_effect_layouts.json` | `583EBB131D08A9CE03FA7375F5F355B02E42987C480ABCE48FD09907105B95C6` |
| `premiere_effect_layouts.md` | `F73BA679E2B91E5C5ECC4B42DE6650052146481A7F71C280AA98A0364B63C120` |
| `premiere_keyboard_locales.md` | `E354EFF57950623C1E2E476780F9CCE0CE72CE97BBD0C6095E1A3CACB4C72AD4` |
| `premiere_plugins_inventory.md` | `A59EF9721BD02686C0E077691AA17ECC6F1C5C6B52458F81182AF1744D830FF6` |
| `premiere_prproj_classes.md` | `88B02EEF881A6FD88BD2601B4026F83639B868F8EB68E7D813A9FC7A35D9B735` |
| `premiere_prproj_classids.json` | `19CE7F8ADCAD4D19AEFF094FD1100A825C091DFF069F8A7C49E01018BDC571CE` |
| `premiere_preset_schema_inventory.md` | `674986C9D8AA6A1CF8E1C0860808A3AD0D291AD1EE4CCCA57C83628278C88265` |
| `premiere_workspace_inventory.md` | `322438831E0077934290F1108E3EB2FC0A1E9F2A33B2970A0AAAA67F31E52001` |

These indexes contain names, identifiers, counts, ranges, and mappings only. They are evidence of the analyzed installation, not proof that every installed module is visible or supported in every Premiere configuration.

## Provenance limitation

The analyzed folder is labelled “Adobe Premiere Portable 2024”. Its licensing and integrity cannot be established from the folder contents. Before publishing compatibility claims or treating the alpha implementation as validated, repeat the same black-box extraction against an official, licensed Premiere Pro 24.x installation and record matching or disputed hashes/counts. This is a mandatory legal and technical gate item.
