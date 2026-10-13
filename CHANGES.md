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
- **쇼츠(세로) 시퀀스** (`shorts.rs`): 시퀀스 메뉴 > 쇼츠(세로) 시퀀스 만들기. 가로 시퀀스를 1080x1920 새 시퀀스에 중첩해 '화면 꽉 채우기(보여줄 위치 선택)' 또는 '블러 배경'으로 맞춥니다. 원본은 그대로 둡니다. AI 도구 `make_shorts`.
- **내보내기 형식 추가** (`op-media/src/encode.rs`, 내보내기 프리셋): Apple ProRes 422 Proxy/LT/422/422 HQ/4444/4444 XQ(.mov), Sony XDCAM HD422 50 Mbps 호환 MXF(.mxf), YouTube 1440p·720p·쇼츠(1080x1920) 프리셋. 모든 코덱을 실제로 내보내 다시 열어 확인하는 테스트(`export_formats.rs`)가 있습니다. XDCAM은 Sony 인증이 아닌 '호환 규격'입니다.
- **효과음·배경음 라이브러리** (`sound_library.rs`, `sounds_panel.rs`, `data/sounds.json`, `tools/build_sound_catalog.py`): 저작권 걱정 없는(CC0) 효과음·배경음 806개 목록. 파일은 설치하지 않고, 듣거나 추가할 때 GitHub에서(커밋 고정 주소) 한 번만 내려받습니다. 창 > 효과음·배경음, AI 도구 `search_sounds`·`add_sound`.
- **모션 템플릿 10종** (`motion_templates.rs`, `templates_panel.rs`): 팝업 제목, 하단 이름표, 강조 박스, 구독·좋아요 알림, 화면 번쩍, 충격 흔들림 글자, 슬라이드 제목, 형광펜 글자, 말풍선 팝, 제목 띠. 텍스트·도형 그래픽 클립에 키프레임을 입혀 한 묶음으로 타임라인에 넣으며, 효과 컨트롤에서 모두 고칠 수 있습니다. AI 도구 `list_motion_templates`·`add_motion_template`.
- **생성형 AI 설정** (환경 설정 > 생성형 AI, ComfyUI 패널; `generative.rs`, `comfy_packs.rs`, `cloud_images.rs`, `secrets.rs`, `dialogs/generative.rs`): 내 PC(그래픽 카드·메모리·디스크) 확인, 로컬 모델 3종(Z-Image Turbo, Krea 2 Turbo, MiniMax H3)의 사양 판정·라이선스 경고·설치, Nano Banana(Gemini)·ChatGPT 이미지 API 키(Windows 자격 증명 관리자에만 저장)와 그림 만들기. 클라우드 호출은 시험용 서버로 요청·응답 처리만 검증했고, 실제 서비스 호출은 키가 없어 시험하지 못했습니다.
- **무음 제거 개선**: 위스퍼를 그래픽 카드(CUDA)용 도우미로 실행하고 large-v3까지 고르고 내려받을 수 있음(`speech.rs`), 내려받은 모델 삭제(환경 설정·무음 제거 창), **문장 밖 빈 공간 모두 삭제** 모드. 자동 자막·문장 편집도 같은 CUDA 경로를 씁니다.
- **CUDA 사용 범위 확대**: 영상 디코딩 NVDEC(CUDA 우선, 안 되면 DirectX·CPU), 프록시 만들기 NVDEC+NVENC, 위스퍼 CUDA. 생성형 AI 창에 사용 현황 표시.
- **리믹스 / 기본 사운드** (`crates/op-remix`, `op-application/src/remix.rs`, `op-ui/src/essential_sound.rs`): 프리미어의 '기본 사운드 > 리믹스'에 해당. 음악 클립 우클릭 > 리믹스 > 리믹스 사용 / 리믹스 속성 / 리믹스 사용 안 함. 22.05 kHz로 줄인 소리에서 크로마·소리 크기·온셋·템포·박자 추적(Ellis 방식)을 구하고, PyMusicLooper(MIT) 방식으로 반복되는 쌍을 찾아(박자와 박자 사이 중간점 후보, 화음 거리, 소리 크기 차, 앞뒤 12박 유사도) 대상 길이에 가장 가까운 '반복 1~2곳'을 골라 클립을 조각으로 나눠 이어 붙입니다(이음새 40 ms 등전력 크로스페이드, 조각은 한 그룹). 이어 붙이기 지점은 상관관계로 ±20 ms 맞춥니다. 실행 취소 한 번으로 되돌아가며, 클립에 효과·볼륨이 있으면 모든 조각이 이어받습니다. 영상과 연결된 오디오·속도를 바꾼 클립은 거부합니다. AI 도구 `remix_music`. 시험: 합성 음악과 실제 곡 8곡으로 PyMusicLooper와 길이 비교, `crates/op-application/tests/remix.rs`.
- **MP3 소리가 앞부분만 읽히던 오류 수정** (`op-media/src/audio.rs`): 대부분의 MP3는 맨 앞에 샘플이 47개뿐인 Xing/Info 프레임이 있는데, 변환기가 첫 프레임 크기에 맞춰 이후 모든 프레임을 잘라 곡 전체가 첫 1~3초만 남았습니다(원본 OpenPremier의 오류; 효과음·배경음 라이브러리의 MP3, 파형, 무음 제거, 리믹스에 영향). 프레임마다 새 출력 프레임을 쓰도록 고쳤고, 깨진 패킷 하나가 소리를 끝내던 것도 건너뛰도록 고쳤으며, 이전 버전이 만든 잘린 캐시는 다시 만들도록 버전을 올렸습니다. 시험: `mp3_conforms_to_its_full_length`.
- **효과 복사·붙여넣기** (`effects_clipboard.rs`): 효과 컨트롤에서 수정한 효과를 복사하고 타임라인 우클릭 > 복사한 효과 붙여넣기로 선택한 클립 전부에 한꺼번에 적용. AI 도구 `copy_effects`·`paste_effects`.
- **시퀀스 설정**: 프로젝트 패널에서 시퀀스를 우클릭 > 시퀀스 설정...에서 크기·프레임 속도 등을 바꿈.
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
