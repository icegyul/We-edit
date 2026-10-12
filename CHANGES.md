# 변경 내역 (CHANGES)

We edit는 **OpenPremier 0.9.0**(커밋 `cda185d`)을 바탕으로 합니다. 아래는 원본과 달라진 점입니다.
(GPL-3.0 §5(a): 수정한 파일과 날짜는 이 저장소의 커밋 이력에 남아 있습니다.)

## 2026-10-13

### 이름·화면
- 프로그램 이름을 **We edit**로, 아이콘을 We edit 로고로 바꿨습니다. 화면에 보이는 "OpenPremier"는 모두 "We edit"입니다.
  (프로젝트 파일 형식 이름과 OTIO 메타데이터 키는 기존 파일과의 호환을 위해 그대로 둡니다.)
- 설정 폴더가 `OpenPremier`에서 `We edit`로 바뀌며, 처음 실행할 때 기존 폴더를 옮깁니다.
- **한국어** 화면 추가 (`crates/op-ui/src/i18n/ko.rs`, 1,730개 문구). 한글 글꼴 대체(맑은 고딕) 추가.

### 편집 기능
- **클립 동기화** (`op-application/src/clipsync.rs`, `audio_sync.rs`): 클립 2개 이상 선택 후 우클릭/클립 메뉴에서 시작·끝·오디오 채널로 맞춤. 소리는 GCC-PHAT로 비교.
- **문장·단어 편집** (`sentences.rs`, `sentences_panel.rs`): Whisper로 받아쓴 단어를 눌러 이동·구간 선택·삭제(모든 트랙 리플 컷)·자막 만들기.
- **Whisper 모델 선택·다운로드** (`whisper_models.rs`, 환경 설정).
- **자막 디자인 12종** (`caption_designs.rs`): 예능·말자막·뉴스·쇼츠·먹방·네온·손글씨 등. 문장 편집 패널(새 자막)과 자막 패널(기존 자막)에서 고르며, 글꼴(OFL, google/fonts)은 처음 쓸 때 한 번 내려받습니다.
- 새 AI 도구(MCP): `sync_by_audio`, `synchronize_clips`, `transcribe_sentences`, `get_sentences`, `delete_sentences`,
  `set_keyframes`, `set_track`, `add_tracks`, `delete_track`, `comfy_*`. `get_timeline`(detail)에 키프레임 표시.

### AI·생성
- **AI 편집 도우미 패널** (`ai_panel.rs`): `claude -p` 또는 `codex exec`로 실행, API 키는 저장하지 않음.
- **AI 계정** 설정 (`ai_accounts.rs`): Claude·ChatGPT(Codex) 로그인 상태·로그인·로그아웃·Codex 설치.
- **ComfyUI** 패널 (`comfy.rs`, `comfy_panel.rs`, `comfy_packs.rs`): 시작/중지, Z-Image Turbo·Krea 2 Turbo·MiniMax H3 생성, Krea 2 모델 설치.

### 문서·기타
- README를 한국어로 다시 썼고, 원본 README는 `docs/OPENPREMIER_README.es.md`로 옮겼습니다.
- `helper/transcribe.py`: faster-whisper 받아쓰기 도우미.
