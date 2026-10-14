<p align="center">
  <img src="assets/icons/we-edit-512.png" width="160" alt="We edit">
</p>

<h1 align="center">We edit</h1>

<p align="center">
  <b>OpenPremier를 기반으로, 한국어 중심의 추가 기능을 넣어 다시 구성한 무료 영상 편집기 (Windows)</b>
</p>

## 소개

**We edit**는 [OpenPremier](https://github.com/xXKuroiKenshiXx/OpenPremier)(Rust로 만든 오픈소스 비선형 영상 편집기,
GPL-3.0)를 바탕으로 만들었습니다. Premiere Pro와 비슷한 패널 구성과 편집 방식은 그대로 두고,
**한국어 사용자가 바로 쓸 수 있도록 기능을 더하고 설정을 새로 구성했습니다.**

원 프로젝트의 저작권은 OpenPremier 기여자들에게 있으며, 이 저장소는 원 프로젝트의 커밋 이력을 그대로 이어받았습니다.
Adobe 및 Premiere Pro는 Adobe Inc.의 상표이며, We edit는 Adobe와 관계없는 독립 프로젝트입니다.

## 더한 기능

| 기능 | 설명 |
|---|---|
| 한국어 화면 | 메뉴·패널·대화 상자 1,700여 개 문구를 한국어로 옮겼고, 한글 글꼴을 함께 지원합니다. 환경 설정 > 언어에서 바꿀 수 있습니다. |
| 문장·단어로 편집 | Whisper가 말을 단어별로 받아쓰면, 단어를 눌러 그 위치로 가고 Shift+클릭으로 구간을 골라 **삭제(모든 트랙 리플 컷)** 하거나 **자막으로 만들 수** 있습니다. 한 번에 실행 취소됩니다. |
| Whisper 모델 선택 | 환경 설정에서 tiny부터 large-v3, 한국어 특화 모델까지 고르고 **바로 내려받을 수** 있습니다. GPU가 있으면 GPU로 돌아갑니다. |
| 클립 동기화 | 타임라인에서 클립 2개 이상을 선택하고 우클릭 > **동기화...** (시작 클립 / 끝 클립 / 오디오 트랙 채널). 서로 다른 카메라·마이크로 찍은 영상을 **소리로 맞춥니다.** |
| AI 편집 도우미 | 프로그램 안 패널에서 대화로 편집합니다. 컷·이동·효과·키프레임·트랙·소리 싱크·문장 삭제·무음 제거·내보내기 등을 도구로 실행합니다. Claude Code 또는 ChatGPT(Codex)에 **로그인해서** 쓰며, 이 프로그램은 API 키를 저장하지 않습니다. |
| AI 계정 설정 | 환경 설정 > AI 계정에서 Claude·ChatGPT 로그인 상태를 확인하고 로그인/로그아웃합니다. |
| ComfyUI 연동 | 패널에서 ComfyUI를 켜고 끄고, **Z-Image Turbo / Krea 2 Turbo (이미지)**, **MiniMax H3 (영상)** 을 만들어 프로젝트로 바로 가져옵니다. 모델이 없으면 설치 버튼이 있습니다. |
| 무음·빈 공간 삭제 | 소리로 무음을 찾고, 위스퍼로 말버릇·반복을 찾으며, **문장 밖 빈 공간을 모두 지울 수** 있습니다. 위스퍼는 NVIDIA 그래픽 카드(CUDA)로 돌고 large-v3까지 고를 수 있습니다. |
| 효과 복사·붙여넣기 | 효과 컨트롤에서 바꾼 모션·불투명도 등을 복사해, 타임라인에서 고른 클립 여러 개에 우클릭 한 번으로 붙여넣습니다. |
| 시퀀스 설정 | 프로젝트 패널에서 시퀀스를 우클릭 > 시퀀스 설정...으로 크기·프레임 속도를 바꿉니다. |
| CUDA 지원 | 위스퍼(CUDA), 영상 디코딩(NVDEC), 내보내기·프록시 인코딩(NVENC), 화면 합성(GPU), ComfyUI(CUDA). 사용 현황은 환경 설정 > 생성형 AI에서 봅니다. |
| 키프레임·트랙 제어 | AI 도구로 효과 키프레임을 찍고, 트랙 잠금·음소거·추가·삭제를 합니다. |
| 한글 자막 디자인 12종 | 예능 노랑·말자막·뉴스·쇼츠 단어 강조·먹방·네온·손글씨·형광펜 등. 문장 편집 창(새 자막)과 캡션 패널(만든 자막)에서 고릅니다. 글꼴은 무료(OFL) 글꼴을 **처음 쓸 때 한 번만** 내려받습니다. |
| 쇼츠(세로) 만들기 | 시퀀스 > 쇼츠(세로) 시퀀스 만들기. 가로 영상을 1080x1920 새 시퀀스에 **화면 꽉 채우기(보여줄 위치 선택)** 또는 **블러 배경**으로 맞춥니다. 원본은 그대로입니다. |
| 내보내기 형식 | H.264·HEVC, YouTube 720p/1080p/1440p/4K/쇼츠 프리셋, **Apple ProRes 6종**(Proxy~4444 XQ, .mov), DNxHR, **Sony XDCAM HD422 호환 MXF**. 모든 형식을 실제로 내보내 다시 열어 확인하는 테스트가 있습니다. |
| 효과음·배경음 | 저작권 걱정 없는(CC0) 효과음·배경음 806개. **설치하지 않고**, 듣거나 추가할 때 한 번만 내려받습니다. 창 > 효과음·배경음. |
| 모션 템플릿 10종 | 팝업 제목·하단 이름표·강조 박스·구독 알림·화면 번쩍·충격 글자·말풍선 등. 키프레임이 들어간 클립 묶음으로 들어가며 효과 컨트롤에서 모두 고칠 수 있습니다. 창 > 모션 템플릿. |
| 리믹스 (기본 사운드) | 타임라인의 음악 클립을 우클릭 > 리믹스 > 리믹스 사용. 곡의 박자·화음·소리 크기를 분석해 **반복되는 부분을 찾고, 그 부분만 한 번 더 반복하거나 건너뛰어** 음악 길이를 자연스럽게 늘리거나 줄입니다. 길이는 1초 단위가 아니라 **반복 구간 길이 단위**(곡마다 다름)로 바뀝니다. 창 > 기본 사운드에서 대상 지속 시간(±5초 버튼)을 정하면 가장 가까운 길이로 맞추고, 이음새마다 짧은 크로스페이드가 들어갑니다. 원래 음악으로 되돌릴 수 있습니다. 모델 내려받기 없이 이 PC에서 바로 분석합니다. |
| 타임라인 편집 | 파일을 끌어 맨 위·아래 점선 칸에 놓으면 새 트랙(V4, A4)이 생기고(파일탐색기에서 끌어와도 놓은 자리에 들어감), 오디오 클립에는 볼륨 선과 점(키프레임)이 보이며, 컷을 더블클릭하면 영상 디졸브와 소리 크로스페이드가 함께 들어갑니다. 트랙 높이는 머리 아래 경계를, 비디오·오디오 영역 높이는 가운데 막대를 끌어 조절합니다. |
| 내보내기 | YouTube 사전 설정은 YouTube 권장 비트 전송률(4K 30fps 45 Mbps 등)과 소리(AAC 384 kbps, 48 kHz)로 자동 설정되고 바꿀 수 있습니다. "대기열에 추가"로 여러 개를 쌓아 두었다가 창 > 내보내기 대기열에서 한꺼번에 내보냅니다. 내보내는 동안 화면 미리보기는 체크박스로 켜고 끕니다(기본 끔, 끄면 더 빠름). |
| 생성형 AI 설정 | 환경 설정 > 생성형 AI. 내 PC(그래픽 카드·메모리·디스크) 확인, 로컬 모델(Z-Image Turbo·Krea 2 Turbo·MiniMax H3)의 사양 판정·라이선스 경고·설치, **Nano Banana(Gemini)·ChatGPT 이미지 API 키**(Windows 자격 증명 관리자에만 저장). |

> 아직 베타입니다. AI 도우미는 Claude Code 또는 Codex에 로그인되어 있어야 답합니다.
>
> 확인 상태: 위 기능은 실제 창과 시험으로 확인했습니다. 다만 **클라우드 이미지(Nano Banana·ChatGPT)는 API 키가 없어 실제 서비스 호출을 시험하지 못했고**(시험용 서버로 요청·응답 처리만 확인),
> 리믹스는 실제 MP3 4곡으로 (이 프로그램의 AI 도구 경로에서) 늘리기·줄이기·되돌리기·실행 취소와 시험 코드를 확인했고, **기본 사운드 패널과 우클릭 메뉴의 버튼 클릭은 실제 창에서 아직 시험하지 못했습니다.**
>
> MiniMax H3는 이 PC(비디오 메모리 8GB)에서 시험하지 않았으며, Sony XDCAM은 Sony 인증이 아닌 호환 규격입니다.
> MiniMax H3·Krea 2의 라이선스에는 사용 지역·상업 이용 제한이 있을 수 있으니 모델 카드를 직접 확인하세요.

## 설치와 실행

Windows 10/11 64비트를 기준으로 합니다.

### 직접 빌드

1. [Rust](https://rustup.rs) 1.98 이상과 Visual Studio Build Tools(C++ 도구)를 설치합니다.
2. 이 저장소를 받고 빌드합니다. FFmpeg 개발 파일은 빌드 중에 자동으로 받습니다.

   ```text
   cargo xtask build --release
   ```

3. 결과 `target\release\openpremier.exe`를 FFmpeg DLL(`third_party\ffmpeg\bin`의 avcodec, avformat, avutil, swresample, swscale)과
   같은 폴더(`<폴더>\app`)에 두고 `We edit.exe`로 이름을 바꿔 실행합니다.

### 받아쓰기 도우미(helper) 설정

문장 편집과 Whisper 자막은 파이썬 도우미를 씁니다. 프로그램 폴더 옆에 `helper` 폴더를 만듭니다
(`<폴더>\app\We edit.exe`, `<폴더>\helper\...`).

```text
python -m venv helper\venv
helper\venv\Scripts\pip install faster-whisper numpy
copy helper\transcribe.py <폴더>\helper\transcribe.py
```

모델은 프로그램의 **환경 설정 > 음성 인식(Whisper)** 에서 내려받습니다. NVIDIA GPU에서 속도를 내려면
`pip install --target <폴더>\helper\cuda nvidia-cublas-cu12`로 cuBLAS를 설치하세요.

## 라이선스

**GNU General Public License v3.0 이상 (GPL-3.0-or-later)** 입니다. 누구나 자유롭게 **사용·복사·수정·배포**할 수 있습니다.
수정하거나 배포할 때는 같은 GPL 조건으로 소스를 공개해야 합니다. 자세한 내용은 [LICENSE](LICENSE)와
[NOTICE.md](NOTICE.md)를 보세요.

## 출처와 감사

- [OpenPremier](https://github.com/xXKuroiKenshiXx/OpenPremier) — 이 프로젝트의 바탕 (GPL-3.0-or-later). 원 README는 [docs/OPENPREMIER_README.es.md](docs/OPENPREMIER_README.es.md)에 있습니다.
- [FFmpeg](https://ffmpeg.org) — 영상·소리 처리.
- [OpenAI Whisper](https://github.com/openai/whisper), [faster-whisper](https://github.com/SYSTRAN/faster-whisper) — 음성 인식 (MIT).
- [PyMusicLooper](https://github.com/arkrow/PyMusicLooper) (MIT) — 음악에서 반복되는 부분을 찾는 방법. 리믹스는 이 방법을 Rust로 다시 구현했습니다(코드 복사 없음). 특징 추출은 [librosa](https://librosa.org)(ISC)의 정의를 따랐습니다.
- [ComfyUI](https://github.com/comfyanonymous/ComfyUI) — 이미지·영상 생성 연동.
- [google/fonts](https://github.com/google/fonts) — 자막 글꼴(SIL OFL 1.1). [Kenney](https://kenney.nl) 등 CC0 소리 —
  [NOTICE.md](NOTICE.md)에 출처와 라이선스를 적었습니다.
- 변경 내역은 [CHANGES.md](CHANGES.md)에 적었습니다.
