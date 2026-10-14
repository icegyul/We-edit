# SPEC-01 자동 마스터링 (AudioMaster 가져오기)

> 사용자 요구: "소리 파일을 타임라인에 쌓으면, 최종 아웃풋할 때 **마스터링이 돼서 녹음실 수준 믹싱**이 되게 하고 싶다. AI가 하면 되지 않을까?"
> 결정: **GitHub의 AudioMaster를 가져와 쓴다.** AI는 값을 제안하고, 실제 처리는 검증된 일반 DSP 코드가 한다.

## 0. 한눈에

- **무엇을**: 내보내기 직전에 시퀀스의 전체 소리 믹스를 분석 → 마스터링 계획(EQ·압축·스테레오·리미터·목표 소리 크기) 생성 → 안전 범위로 검증 → 적용 → 결과 측정과 경고 표시.
- **어떻게**: AudioMaster(`mastering-core`)의 분석·계획·검증·DSP를 새 크레이트 `op-master`로 **반입**하고, 내보내기(`export.rs`)에 끼운다. 사용자는 내보내기 대화상자의 체크박스 하나 또는 "마스터링" 패널로 쓴다.
- **AI의 역할**: 우리 AI 패널(Claude/ChatGPT 로그인)이 MCP 도구로 분석 값을 읽고 `MasteringPlan` JSON을 제안한다. **모든 값은 `validate`가 안전 범위로 자른다.** AI는 소리를 직접 만지지 않는다(AudioMaster의 설계 원칙과 같다).
- **하지 않는 것(정직하게)**: 나쁜 녹음·소음·클리핑을 고치지 못한다(그건 `02`의 음성 정리가 맡는다). 트랙별 밸런스는 `02`의 덕킹·소리 크기 맞추기가 맡는다. **"녹음실급"은 보장하지 않으며** 소리 크기·피크 안전·음색 균형·일관성을 자동으로 맞춰 주는 것까지다. 최종 판정은 사람의 귀.

### 소리 처리 3층 구조 (전체 그림)
| 층 | 하는 일 | 어디서 | 명세 |
|---|---|---|---|
| 1 클립 정리 | 잡음 제거, 험·치찰음, 역할별 보정 | 클립 효과 | `02` |
| 2 믹스 | 소리 크기 맞추기, 목소리 위에서 배경음 자동 줄이기 | 볼륨 선(키프레임) | `02` |
| **3 마스터** | **전체 믹스의 음색 균형·압축·피크·목표 소리 크기** | **내보낼 때(이 문서)** | **`01`** |

## 1. 왜 AudioMaster인가 (조사 근거)

- 저장소: `https://github.com/KEYHAN-A/AudioMaster`, 기준 커밋 `6496be8ef8f4fda964c54c9c402d38b12dcb076f`(2026-09-01, 버전 1.2.1). **라이선스 `GPL-3.0-or-later`** — We edit와 같아서 코드 반입이 가능하다.
- Rust 로컬 우선 엔진: BS.1770 소리 크기(통합·단기·순간), LRA, 샘플/트루 피크, 크레스트, DC, 클리핑, 스테레오 폭·상관, 7밴드 분석, **계획 검증(`validate_params`)**, EQ·연동 압축·M/S 폭·룩어헤드 리미터·트루피크 강제, 경고 코드.
- LLM은 계획 JSON만 만들고 **엔진이 값을 검증한 뒤에만 렌더**한다 → 우리 요구(AI가 하되 안전하게)와 같은 구조.
- 약점(코드로 확인): 별 4개의 초기 프로젝트, **음악용 보수적 설정**(EQ ±1.5 dB, 압축비 1.25~2.2), 전체 파일을 메모리에 올림(256 MiB 상한), 주파수 분석이 거친 방식(Goertzel 8탐침, 최대 64창), 모노·스테레오만 지원, 소리 크기 맞춤 후 리미터가 평균을 낮출 수 있는데 보정은 한 번뿐. → 6~8장에서 보완한다.
- 쓰지 못하는 대안: Diff-MST(비상업 CC BY-NC-SA), DeepAFx-ST(Adobe 연구용 라이선스). Matchering은 Python(GPL-3.0) — 참조곡 방식은 AudioMaster의 참조 계획(`reference`)으로 대신하고, 필요하면 알고리즘만 나중에 Rust로.

## 2. 가져오는 것 / 안 가져오는 것

원본 경로는 `crates/mastering-core/src/` 기준.

| 원본 | 처리 | 새 위치(`crates/op-master/src/`) | 바꾸는 점 |
|---|---|---|---|
| `types.rs` | 가져옴 | `types.rs` | `AudioAnalysis`의 `metadata.path/format` 제거, `Profile` 추가. 나머지(`FrequencyBands`, `MasteringParams`, `EqBand`, `CompressionParams`, `LimiterParams`, `StereoParams`, `MasteringPlan`, `MasteringWarning`)는 유지. `Backend/AiProvider/Preset/AudioFormat` 등 CLI·클라우드용은 제외 |
| `analysis/decode.rs` | **`DecodedAudio` 구조만** | `audio.rs` | symphonia 디코드 삭제. `Audio { samples: Vec<f32>, rate, channels }`(인터리브) |
| `analysis/metrics.rs` | 가져옴 | `analysis.rs` | `path` 인자 제거. `compute_frequency_bands`를 `realfft` 평균 스펙트럼(Welch)으로 교체(경계 7개는 그대로). 나머지 측정은 유지하되 `ebur128`과 교차 시험 |
| `dsp.rs` | 일부 | `validate.rs`, `dsp.rs` | `validate_params`+`clamp_with_warning`→`validate.rs`. `apply_eq/make_eq_filter/apply_compression/compressor_reduction/apply_stereo/apply_low_end_mono_guard/apply_resonance_and_sibilance_guard/apply_gain/apply_lookahead_limiter/oversampled_frame_peak/enforce_true_peak_ceiling/sanitize`→`dsp.rs`. `render_wav*`, `render_wav_streaming_ffmpeg`, `measure_streaming_lufs`, `probe_streaming_timing`, `write_wav`(+tpdf)는 **제외**(우리는 인코더가 쓴다) |
| `backends/native.rs` | 계획 함수만 | `plan.rs` | `create_mastering_plan`만. `tokio`·`MasteringOptions` 제거, `Options`·`Profile`로 대체 |
| `pipeline/mod.rs` | 경고 판정만 | `report.rs` | `LOUDNESS_TARGET_MISS`(목표±1.0 LU), `CLIPPED_SAMPLES`, `DC_OFFSET`(>0.005), `PHASE_RISK`(상관<−0.1), `ENGINE_ADJUSTMENT` 판정 로직만 |
| `control.rs` | 개념만 | `control.rs` | 우리 취소·진행 방식(`Arc<AtomicBool>` + 콜백)으로 새로 작성 |
| `qualification.rs` | 아이디어만 | `tests/` | 측정값 검증을 시험으로 |
| `backends/ai.rs`의 **프롬프트·JSON 계약** | 문구만 | `07`/`ai_panel.rs` | HTTP·API 키 코드는 **제외**. 시스템 프롬프트의 JSON 계약(버전 1, `deny_unknown_fields`)만 우리 AI 설명에 사용 |
| `cloud.rs`, `backends/ai.rs`(HTTP), `backends/local_ml.rs`, `backends/matchering.rs`, `gpu.rs`, `config.rs`, `cache.rs`, `album.rs`, `error.rs`, `src-tauri`, `python/`, `website/` | **안 가져옴** | — | API 키 직접 호출 금지(규칙 6), 파이썬·GPU·앨범·앱 UI는 불필요 |

### 라이선스·출처 규칙
1. 가져온 각 파일 맨 위 주석(예시):
   ```rust
   // Derived from AudioMaster (https://github.com/KEYHAN-A/AudioMaster), crates/mastering-core/src/dsp.rs,
   // commit 6496be8ef8f4fda964c54c9c402d38b12dcb076f, Copyright (C) KEYHAN STUDIO, GPL-3.0-or-later.
   // Changes for We edit: input is an in-memory Audio, file I/O and ffmpeg fallback removed, ...
   ```
2. `NOTICE.md`에 "AudioMaster — KEYHAN-A — GPL-3.0-or-later — 자동 마스터링 분석·계획·DSP(`op-master`)" 한 줄.
3. `docs/dev/vendor/AUDIOMASTER.md`를 만들어 **파일별로 무엇을 바꿨는지** 적는다(나중에 업스트림 변경을 사람이 비교할 수 있게).
4. `op-master/Cargo.toml`의 `license = "GPL-3.0-or-later"`(워크스페이스 값 그대로).
5. 조사용 복사본(`D:\## APP\We edit\scratch-am\am`)은 임시다. 다시 받을 때:
   ```bash
   git clone --depth 1 https://github.com/KEYHAN-A/AudioMaster.git am
   cd am && git fetch --depth 1 origin 6496be8ef8f4fda964c54c9c402d38b12dcb076f && git checkout 6496be8
   ```
   **내려받은 코드는 읽기만 하고 실행·빌드하지 않는다**(우리 크레이트로 옮긴 뒤 우리 도구로 시험).

## 3. 새 크레이트 `crates/op-master`

```
crates/op-master/
  Cargo.toml          # serde, serde_json, thiserror, realfft(workspace); dev: ebur128, tempfile
  src/lib.rs          # 공개 API 재노출
  src/types.rs        # 데이터 형식 (4장)
  src/audio.rs        # Audio { samples: Vec<f32> (인터리브), rate: u32, channels: u16 }
  src/analysis.rs     # analyze(&Audio) -> Analysis
  src/validate.rs     # validate(params, rate, target_lufs, no_limiter) -> Validated
  src/dsp.rs          # EQ·압축·스테레오·가드·리미터·트루피크
  src/plan.rs         # plan(analysis, reference, &Options) -> MasteringParams (프로파일 보정 포함)
  src/render.rs       # master(), master_to_target()
  src/report.rs       # check(post, target, lossy) -> Vec<Warning>
  src/control.rs      # Control { cancel, progress }
  tests/              # 시험 (9장)
```

워크스페이스 `Cargo.toml`: `members`에 자동 포함(`crates/*`인지 확인), `[workspace.dependencies]`에 `op-master = { path = "crates/op-master" }`와 `ebur128 = "0.1"`(dev용) 추가. `op-application`이 `op-master.workspace = true`로 의존.

### 공개 API (시그니처 확정안)
```rust
pub struct Audio { pub samples: Vec<f32>, pub rate: u32, pub channels: u16 }   // 1 또는 2채널만

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Profile { Music, Speech, Mixed }          // 기본은 Mixed (말+음악)

#[derive(Clone, Debug, PartialEq)]
pub struct Options { pub target_lufs: f64, pub no_limiter: bool, pub lossy_delivery: bool, pub profile: Profile }

pub fn analyze(audio: &Audio) -> Analysis;
pub fn plan(analysis: &Analysis, reference: Option<&Analysis>, opt: &Options) -> MasteringParams;
pub fn validate(p: MasteringParams, rate: u32, target_lufs: f64, no_limiter: bool) -> Result<Validated, MasterError>;
pub fn master(audio: &mut Audio, p: &MasteringParams, ctl: &Control) -> Result<Vec<String>, MasterError>;
pub fn master_to_target(audio: &mut Audio, p: &MasteringParams, tol_lu: f64, ctl: &Control) -> Result<Vec<String>, MasterError>;
pub fn check(post: &Analysis, target_lufs: f64, lossy: bool) -> Vec<Warning>;
```
`Control`: `cancel: Arc<AtomicBool>`, `progress: Option<Arc<dyn Fn(Stage, f32) + Send + Sync>>`. 단계(`Stage`): 분석 → 음색 → 다이내믹스 → 소리 크기 → 리미터 → 확인.

## 4. 데이터 형식

`MasteringPlan`(AudioMaster 그대로, `deny_unknown_fields`, `schema_version = 1`):
```json
{ "schema_version": 1,
  "params": {
    "eq": [ {"frequency": 100.0, "gain_db": 0.5, "q": 0.707, "band_type": "low_shelf"} ],
    "compression": {"threshold_db": -18.0, "ratio": 2.0, "attack_ms": 10.0, "release_ms": 100.0, "knee_db": 6.0, "makeup_gain_db": 0.0},
    "limiter": {"enabled": true, "ceiling_db": -1.0, "release_ms": 80.0},
    "stereo": {"width": 1.0, "balance": 0.0},
    "target_lufs": -14.0 } }
```
`band_type`: `low_shelf, high_shelf, peak, low_pass, high_pass`.

**검증 범위(`validate`, AudioMaster 코드 그대로 — 범위를 넘으면 자르고 경고)**
| 항목 | 최소 | 최대 |
|---|---|---|
| EQ 주파수 | 15 Hz | 샘플레이트×0.49 |
| EQ 이득 | −12 dB | +12 dB |
| EQ Q | 0.1 | 12 |
| 압축 threshold | −60 dB | 0 dB |
| 압축 ratio | 1 | 20 |
| 압축 attack / release | 0.1 / 5 ms | 200 / 2000 ms |
| 압축 knee / makeup | 0 / −12 dB | 24 / +12 dB |
| 리미터 천장 | −6.0 dBTP | −0.1 dBTP |
| 리미터 release | 5 ms | 1000 ms |
| 스테레오 폭 / 밸런스 | 0 / −1 | 2 / +1 |
| 목표 소리 크기 | −24 LUFS | −5 LUFS |

NaN·무한대는 오류. 샘플레이트 8 kHz 미만은 오류. (AI가 낸 목표 LUFS는 **요청한 목표로 덮어쓴다** — 원본 동작 유지.)

**프로젝트 저장(`.wedalur`)**: `Sequence.master_plan: Option<MasterPlanRecord>` — `op-core/src/model.rs`의 `Sequence`(마스터 `master: Vec<Component>` 근처). `#[serde(default, skip_serializing_if = "Option::is_none")]`로 예전 파일 호환.
```rust
pub struct MasterPlanRecord { pub schema_version: u16, pub profile: Profile, pub params: MasteringParams,
                              pub before: Option<Analysis>, pub made_unix: u64 }
```
적용·지우기는 `seq_edit`(되돌리기 한 단계).

## 5. 처리 순서 (AudioMaster 그대로 + 우리 변경)

AudioMaster `render_wav_with_control`의 순서:
1. `sanitize`(NaN·무한대 제거) → 2. **EQ**(고역통과 25 Hz, 저음 선반 100 Hz, 피크 350·1200·3000·5200 Hz, 고음 선반 10 kHz; biquad) → 3. **공명·치찰 가드**(5.5 kHz 이상을 최대 −2 dB 동적으로 줄임) → 4. **저역 모노 가드**(100 Hz 이하의 side 성분 제거) → 5. **압축**(좌우 연동 피크 검출, 소프트 니, 어택·릴리스) → 6. **스테레오**(M/S 폭, 밸런스) → 7. 중간 분석 후 **소리 크기 이득**(목표−현재, ±24 dB 제한) → 8. **룩어헤드 리미터**(5 ms, 4배 오버샘플 피크 예측, Catmull-Rom) → 9. **트루피크 강제**(초과하면 전체 이득을 낮춤).

우리 변경:
| # | 변경 | 이유 |
|---|---|---|
| D1 | 입력·출력이 파일이 아니라 메모리 `Audio` | 우리는 믹서 출력을 받고 인코더로 보냄 |
| D2 | 256 MiB 상한·ffmpeg 명령줄 폴백 삭제. 대신 **길이 상한 안내**(7.2) | ffmpeg.exe를 배포하지 않음 |
| D3 | 주파수 대역 분석을 `realfft` 평균 스펙트럼으로 교체 | 정확도. 7밴드 값의 의미·경계는 동일 |
| D4 | **반복 보정**: 리미터 뒤 측정 → 목표와 ±0.5 LU 넘게 다르면 이득을 조정해 7~9를 최대 3회 재실행(`master_to_target`) | 리미터·트루피크 트림이 평균을 낮추는 문제 |
| D5 | `Profile`(말/말+음악/음악)에 따른 계획 보정(6.2) | 원본은 음악용 |
| D6 | 손실 인코딩(AAC) 때 천장 −1.2 dBTP(원본 규칙 유지) | AAC가 피크를 올릴 수 있음 |
| D7 | 경고 메시지를 한국어로(`ko.rs`) | 사용자 화면 |
| D8 | 모노·스테레오 외(5.1)는 **마스터링 건너뜀 + 알림** | 원본 미지원 |
| D9 | tokio/anyhow/reqwest/symphonia 의존 제거, `thiserror` | 우리 의존성 규칙 |

## 6. 계획 만들기

### 6.1 규칙 기반 (기본, AI 없이도 동작)
`plan.rs` = AudioMaster `create_mastering_plan`:
- 저음 이득 = `clamp((bass − mid) × −0.12, ±1.5)`, 고음 = `clamp((brilliance − mid) × −0.08, ±1.25)`, 탁함(저중음 > 중음+3 dB)이면 350 Hz −1 dB.
- 압축 ratio: crest > 16 → 2.2, > 11 → 1.7, 그 밖 1.25. threshold = `clamp(rms + 4, −30, −8)`. attack 12 ms(crest>14면 25), release 110 ms(LRA>12면 180).
- 폭: 위상 상관 < 0 → 0.85, 폭 > 1.35 → 0.9, 아니면 1.0. 리미터 천장 −1.0(손실 −1.2), release 80 ms.
- **참조곡**(선택): 참조 분석과 비교해 저음·고음·중간 대역·압축·폭을 ±2 dB 안에서 조금씩 이동(원본 로직 그대로).

### 6.2 프로파일 보정 (우리 추가 — **시작값, 청취로 확정**)
말 위주 영상에 음악용 값을 그대로 쓰면 목소리가 얇아지거나 저역이 남는다. `Profile`에 따라 `plan()`의 결과를 덮는다. 아래 숫자는 **시작값이며 사용자 청취(11장)로 확정**한다. 확정되면 이 표를 갱신한다.
| 항목 | Music | Mixed(기본) | Speech |
|---|---|---|---|
| 고역통과 | 25 Hz | 40 Hz | 70 Hz |
| 저음 선반 이득 한도 | ±1.5 | ±1.0 | 0 ~ −1.0 |
| 350 Hz 탁함 | −1 (조건부) | −1.5까지 | −1.5까지 |
| 3 kHz 명료도 | 0 | 0 ~ +0.5 | 0 ~ +1.0 (upper_mid가 mid보다 6 dB 넘게 낮을 때) |
| 압축 ratio | 1.25~2.2 | 1.5~2.4 | 2.0~2.8 |
| 압축 attack / release | 12~25 / 110~180 | 10 / 120 | 8 / 120 |
| 목표 소리 크기 | −14 | −14 | −14(유튜브) / −16(팟캐스트) |
| 폭 | 1.0 | 1.0 | 1.0(모노 소스면 건드리지 않음) |

규칙: 보정 후에도 모든 값은 `validate`를 통과해야 하며, EQ 한 대역의 이동은 **±3 dB 이내**.

### 6.3 AI 계획 (우리 AI 패널 + MCP)
1. AI가 `master_analyze`로 분석 JSON을 받는다(소리 크기·LRA·피크·크레스트·상관·클리핑·7밴드).
2. AI가 사용자의 말("더 따뜻하게", "더 크게")과 분석을 보고 `MasteringPlan` JSON(버전 1)을 만든다. 시스템 프롬프트의 계약은 AudioMaster `SYSTEM_PROMPT`와 같다(EQ는 보통 ±3 dB, band_type 5종, 버전 1).
3. `master_apply`가 **JSON 파싱(알 수 없는 필드·버전 불일치 거부) → `validate`(자름 + 경고) → 적용**. AI가 값을 어겨도 소리가 망가지지 않는다.
4. AI는 API 키를 받지 않는다(규칙 6). AudioMaster의 OpenAI/Anthropic/LM Studio/Ollama HTTP 호출 코드는 **가져오지 않는다**.

## 7. 우리 프로그램에 붙이는 지점

### 7.1 믹스다운 만들기 `render_mixdown`
`op-application/src/export.rs`의 소리 루프(영상 프레임마다 `m.render(project, sid, t, 1.0, &mut buf, source)`를 8192프레임 단위로 호출)를 **함수로 뽑아낸다**:
```rust
pub fn render_mixdown(project: &Project, s: &ExportSettings, a: &AudioSettings,
                      source: &dyn AudioSource, cancel: &AtomicBool,
                      progress: impl Fn(f32)) -> Result<Vec<f32>, String>   // 인터리브 f32, a.rate/a.channels
```
기존 경로와 **표본 단위로 동일**해야 한다(시험 T6). 이 함수는 마스터링 분석·미리듣기·"소리 크기 맞추기"(02)가 함께 쓴다.

### 7.2 내보내기 통합
- `ExportSettings`에 `pub master: Option<MasterRequest>` 추가(`Clone + PartialEq`). `MasterRequest { profile, target_lufs, no_limiter, params: Option<MasteringParams> }` — `params`가 있으면 저장된 계획 사용, 없으면 내보낼 때 자동 계획.
- `run()` 흐름(`s.audio.is_some() && s.master.is_some()`):
  1. 기존대로 `media.wait_audio(...)`로 소리 변환 대기.
  2. `progress.phase = "소리 분석"` → `render_mixdown` → `Audio`.
  3. 채널 1·2가 아니면 알림 후 건너뜀(D8).
  4. `analyze` → `plan`(또는 저장된 params) → `validate` → `master_to_target` → `analyze`(결과) → `check` 경고 수집.
  5. 영상 프레임 루프에서 `m.render(...)` 대신 **마스터링된 버퍼에서 `audio_done` 위치부터 잘라 `Packet::Audio`로 보냄**. 소리 길이·프레임 정렬은 기존 `audio_total` 계산 그대로.
  6. 끝나면 `Progress`에 `master_report: Option<MasterReport>`(전/후 분석, 경고)를 담아 UI·MCP가 보여 준다.
- **진행 표시**: `Progress.preparing`을 단계 문자열로 확장(`phase: String`: "소리를 준비하는 중" → "소리를 분석하는 중" → "마스터링 중" → 영상 렌더). 취소는 각 단계에서 `cancel` 확인.
- **메모리**: 버퍼 크기 = 초 × 샘플레이트 × 채널 × 4바이트. 48 kHz 스테레오 20분 ≈ 460 MB. **길이 상한 30분**(초과하면 "마스터링은 30분까지 지원합니다. 구간을 나눠 내보내세요" 안내하고 마스터링 없이 내보낼지 묻는다). 상한은 설정값으로 두고, 램이 부족하면 임시 파일(메모리 맵) 처리는 2차 과제.
- **내보내기 대기열**(`export_queue.rs`)은 `ExportSettings`를 그대로 저장하므로 `master`도 같이 저장된다. 추가 작업 없음(시험 T9에서 확인).
- 마스터링을 끄면(`master: None`) 기존 경로와 **비트 단위로 동일**해야 한다(T8).

### 7.3 UI
**(1) 내보내기 대화상자** (`op-ui/src/dialogs/export.rs`, `ExportForm`에 필드 추가)
- `☑ 소리 자동 마스터링` (기본 꺼짐)
  - 대상: `유튜브·쇼츠 (-14 LUFS)` / `팟캐스트 (-16 LUFS)` / `방송 (-23 LUFS)` / `직접 입력`
  - 종류: `말+음악(권장)` / `말 위주` / `음악 위주`
  - 저장된 계획이 있으면 `☑ 저장된 마스터링 계획 사용` 표시.
- 유튜브 프리셋을 고르면 마스터링 대상이 `-14 LUFS`로 따라 바뀌되 사용자가 고칠 수 있다(영상 비트레이트 자동 설정과 같은 방식).
- 내보내기가 끝나면 상태줄에 `마스터링: -23.4 → -14.1 LUFS, 최대 피크 -1.2 dBTP` + 경고가 있으면 개수.

**(2) 마스터링 패널** (새 `op-ui/src/master_panel.rs`, `Panel::Master`, 제목 "마스터링")
- 대상 구간: 시퀀스 전체 / 입·출력 표시 구간. 목표·종류 선택, 참조곡 파일(선택).
- 버튼: `분석하기`(전 분석 표) → `계획 만들기`(규칙) → 계획 표(EQ 7행: 주파수·이득·Q / 압축 6개 / 리미터 / 폭 / 목표; 값은 편집 가능, 범위 밖이면 즉시 잘려서 표시) → `미리듣기(30초, 전/후)` → `시퀀스에 저장` / `지우기`.
- **전/후 표**: 통합 LUFS, 단기 최대, LRA, 트루 피크, 크레스트, 스테레오 상관, 클리핑 수, DC + 7밴드 막대.
- 경고 목록(한국어): 아래 문구를 사용한다.
| 코드 | 표시 문구(안) |
|---|---|
| `LOUDNESS_TARGET_MISS` | 목표 소리 크기와 {차이} LU 다릅니다 (목표 {목표}, 결과 {결과}) |
| `CLIPPED_SAMPLES` | 소리가 찢어진(클리핑) 부분이 {개수}곳 있습니다. 원본 녹음을 확인하세요 |
| `DC_OFFSET` | 소리에 직류 성분이 있습니다({값}) |
| `PHASE_RISK` | 좌우 위상이 반대인 부분이 있어 모노 재생에서 소리가 줄 수 있습니다 |
| `ENGINE_ADJUSTMENT` | 안전 범위로 조정한 값: {항목} |
- 패널 추가 절차는 `00` 8장 "새 패널" 행. 문구는 `ko.rs`에 직접 한국어로.

**(3) 미리듣기(A/B)**: 시퀀스의 30초(재생헤드부터 또는 선택 구간)를 `render_mixdown` → (후) `master_to_target` → 임시 WAV로 써서 기존 **소리 미리듣기 플레이어**(`sound_library.rs`/`SoundPreview` 캐시)로 재생. `전 / 후` 두 버튼으로 번갈아 듣기. 구현 전에 그 플레이어가 임의 WAV를 재생하는지 `grep`으로 확인하고, 안 되면 `op-audio`에 단순 버퍼 재생을 추가한다.

### 7.4 MCP 도구 (`op-mcp/src/tools.rs`: 정의 + `run` + `live` 분기)
| 도구 | 인자 | 결과 |
|---|---|---|
| `master_analyze` | `start?`, `end?`(초) | `Analysis` JSON + 경고 |
| `master_plan` | `target_lufs?`, `profile?`("music"\|"speech"\|"mixed"), `reference_path?`, `no_limiter?` | 규칙 기반 `MasteringPlan` JSON |
| `master_apply` | `plan?`(JSON, 없으면 규칙 계획), `target_lufs?`, `preview?`(bool) | 검증된 계획 + 경고 + (preview면 전/후 WAV 경로). 시퀀스에 저장 |
| `master_clear` | — | 저장된 계획 삭제 |
`export` 도구에 `master`(bool)·`master_target_lufs` 인자 추가. AI 패널 프롬프트(`ai_panel.rs`)에 한 줄: "소리를 마스터링하려면 master_analyze로 측정하고 master_plan·master_apply를 쓰세요. 값은 자동으로 안전 범위로 제한됩니다."

### 7.5 2단계(선택, 후순위): 마스터 버스 이펙트로 굽기
재생 중에도 마스터링된 소리를 듣고 싶을 때. `Sequence.master` 삽입 효과로 `plan`을 변환한다. 필요한 새 실시간 효과: `op.audio.master_eq`(7 biquad), `op.audio.master_guard`(공명·저역 모노), `op.audio.master_tp_limiter`(룩어헤드 + 4배 오버샘플 피크), 압축에 knee. 기존 `op.audio.parametric_eq`(5밴드)·`compressor`(니 없음)·`hard_limiter`(오버샘플 없음)로는 같은 소리가 나지 않으므로 **새로 만든다**. 1단계(내보낼 때 적용)가 사용자 확인을 받은 뒤에 결정한다.

## 8. 알려진 한계 · 위험 (이 문서가 약속하지 않는 것)
- **음악용 보수적 설정**이라 큰 변화를 기대하면 안 된다(EQ ±1.5 dB, 압축비 ≤2.2). 말 위주 보정(6.2)은 **청취로 확정하기 전까지 시작값**이다.
- 원본 소리의 문제(소음·클리핑·목소리 대 음악 밸런스)는 마스터링으로 고쳐지지 않는다 → `02`.
- 5.1 서라운드 미지원. 30분 초과 미지원(1차).
- AudioMaster는 **우리가 직접 실행해 검증하지 않았다**(읽기만 함). 반입 후 9장 시험으로 측정값을 `ebur128`과 맞춰 보기 전에는 정확하다고 말하지 않는다. 원본 `qualification/`에는 예시 매니페스트만 있고 기준 파일이 없다.
- AAC 손실 인코딩이 트루 피크를 약간 올릴 수 있다 → 천장 −1.2 dBTP, 내보낸 파일을 다시 측정하는 시험(T7).
- 사람마다 "녹음실급"의 기준이 다르다 → 11장 체크리스트로 사용자 본인이 판정.

## 9. 합격 기준 (자동 시험)
`crates/op-master/tests/*.rs` 와 `crates/op-application/tests/export_master.rs`. 합성 신호: 핑크 노이즈, 변조 잡음(말 비슷한 포락선), 1 kHz 사인, 풀스케일 샘플 간 피크 신호.

| # | 이름 | 입력 → 기대 |
|---|---|---|
| T1 | `analysis_matches_ebur128` | 핑크 노이즈·변조 잡음·사인(−23, −30, −14 LUFS 수준) → 통합 LUFS 차이 ≤ 0.1, LRA 차이 ≤ 0.5, 트루 피크 차이 ≤ 0.2 dB (`ebur128` 기준) |
| T2 | `bands_follow_tilt` | 기울기가 알려진 핑크/화이트 노이즈 → 7밴드 값이 이론 기울기와 ±1.5 dB 이내(Welch 교체 검증) |
| T3 | `validate_clamps_and_warns` | 범위 밖 계획 → 모든 항목이 표의 한계로 잘리고 경고 발생. NaN → 오류 |
| T4 | `plan_json_contract` | 알 수 없는 필드·`schema_version` 99 → 거부. 정상 JSON 왕복 동일 |
| T5 | `low_end_mono_guard` | 50 Hz side 신호 → 20 dB 이상 감소, 2 kHz side는 ±0.5 dB |
| T6 | `limiter_true_peak` | 샘플 간 피크 0 dBFS 초과 신호 → 결과 트루 피크 ≤ 천장 + 0.1 dB, 클리핑 0 |
| T7 | `master_hits_target` | −30 LUFS 변조 잡음 → 목표 −14: 결과 −14 ± 0.5 LU, 트루 피크 ≤ −1.0 dBTP, 클리핑 0. **AAC로 인코딩·재디코드한 뒤에도** 소리 크기 ±0.7 LU, 트루 피크 ≤ −0.5 dBTP |
| T8 | `master_off_is_identical` | `master: None` 내보내기 → 변경 전 코드의 소리와 표본 단위 동일 |
| T9 | `mixdown_equals_export_audio` | `render_mixdown` 결과 == 기존 내보내기 소리 경로 출력 |
| T10 | `export_with_master` | 소리가 있는 작은 프로젝트 내보내기(마스터링 켬) → 파일 소리 크기가 목표 ±0.7 LU, 영상 프레임 수 불변, 진행 단계 문자열 순서 확인. 대기열에 넣어도 동일 |
| T11 | `cancel_during_master` | 마스터링 중 취소 → 파일 삭제, 오류 아님 |
| T12 | `speech_profile_is_gentle` | 변조 잡음에 Speech 프로파일 → 평균 게인 리덕션 ≤ 4 dB, 최대 ≤ 10 dB, 모든 EQ 이동 ≤ ±3 dB |
| T13 | `unsupported_layout` | 5.1 마스터 → 마스터링 건너뜀 + 알림, 내보내기는 성공 |
| T14 | `project_roundtrip` | 계획 저장 → `.wedalur` 저장·열기 → 같은 계획. 예전 파일(필드 없음)이 열림 |

## 10. 작업 순서 (체크리스트)
- [ ] **T-1** `op-master` 골격 + 파일 반입(복사 → 컴파일 → 헤더·`NOTICE.md`·`docs/dev/vendor/AUDIOMASTER.md`) [S]
- [ ] **T-2** `types/validate/analysis` 이식, Welch 밴드, 시험 T1·T2·T3·T4 [M]
- [ ] **T-3** `dsp` 이식(EQ·압축·스테레오·가드·리미터·트루피크), 시험 T5·T6 [M]
- [ ] **T-4** `plan`(+프로파일 표 6.2), 시험 T12 [S]
- [ ] **T-5** `master`·`master_to_target`·`check`, 시험 T7(인코딩 전) [S]
- [ ] **T-6** `render_mixdown` 추출, 시험 T9 [S]
- [ ] **T-7** 내보내기 통합(진행 단계·취소·길이 상한·5.1 건너뜀), 시험 T7(AAC)·T8·T10·T11·T13 [M]
- [ ] **T-8** `Sequence.master_plan` 저장·되돌리기, 시험 T14 [S]
- [ ] **T-9** 내보내기 대화상자 UI [S]
- [ ] **T-10** 마스터링 패널 UI + 한국어 문구 + ko/es 번역 [M]
- [ ] **T-11** MCP 도구 4개 + `export` 인자 + AI 프롬프트 줄 [S]
- [ ] **T-12** 30초 A/B 미리듣기 [M]
- [ ] **T-13** `CHANGES.md`·`README.md`·`NOTICE.md` 갱신, 전체 시험, 릴리스 빌드, **시험용 실제 창에서 내보내기 실행**, 설치, 커밋 [S]
- [ ] **T-14 (사람)** 사용자 청취 확인 → 프로파일 값(6.2) 확정 [사용자]
- [ ] **T-15 (선택)** 7.5 마스터 버스 이펙트로 굽기 — T-14 결과를 보고 결정

## 11. 사용자 청취 체크리스트 (T-14, 자동 시험으로 대신할 수 없음)
사용자 본인의 영상 3편(말 위주 1, 말+음악 1, 음악 많은 1)으로 마스터링 끔/켬을 번갈아 듣고 항목별로 기록한다.
1. 목소리 선명도(뭉침·얇음) 2. 저역(웅웅거림) 3. 치찰음(ㅅ) 4. 음악 대비 목소리 크기 5. 펌핑(압축이 숨 쉬는 느낌) 6. 클리핑·찢어짐 7. 영상 전체 소리 크기 일관성 8. 스피커·이어폰·스마트폰 세 곳에서 들어 보기.
9. **유튜브에 비공개로 올려 "통계"의 콘텐츠 라우드니스가 0 dB 근처(= -14 LUFS 부근)** 인지 확인(유튜브의 기준값은 바뀔 수 있으므로 프리셋 값은 설정에서 고칠 수 있게 둔다).
결과를 `docs/dev/vendor/AUDIOMASTER.md`의 "청취 기록"에 남기고, 6.2 표의 값을 확정한다.

## 12. 사용자에게 보고할 때
- 한 것: 분석·계획·검증·마스터링을 내보내기에 붙였다 / 시험한 것: T1~T14 중 통과 목록 / **직접 들어 봐야 하는 것: 11장**.
- 쓰지 말 것: "녹음실급이 됩니다", "AI가 믹싱해 줍니다". 쓸 것: "목표 소리 크기·피크 안전·음색 균형을 자동으로 맞춥니다. 나쁜 녹음은 고치지 못합니다."
