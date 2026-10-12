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
| 키프레임·트랙 제어 | AI 도구로 효과 키프레임을 찍고, 트랙 잠금·음소거·추가·삭제를 합니다. |

> 아직 베타입니다. AI 도우미는 Claude Code 또는 Codex에 로그인되어 있어야 답합니다.

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
- [ComfyUI](https://github.com/comfyanonymous/ComfyUI) — 이미지·영상 생성 연동.
- 변경 내역은 [CHANGES.md](CHANGES.md)에 적었습니다.
