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
- **미리보기가 느리던 원인 수정** (`op-media/src/video.rs`): NVIDIA 그래픽 카드(NVDEC)는 4:2:2 10비트 H.264(소니 카메라 등)를 하드웨어로 풀지 못하는데, 프로그램이 카드를 붙인 채 열어 FFmpeg가 조용히 소프트웨어 디코딩으로 돌아가면서 **스레드 1개**로 풀었습니다(4K 4:2:2 10비트 140 Mbps 시험 영상: 초당 10프레임). 카드가 풀 수 없는 영상(4:2:0이 아니거나 H.264 8비트가 아님)은 처음부터 프로세서가 모든 코어로 풀게 했습니다(같은 영상 초당 59프레임). 카드가 풀 수 있는 영상은 그대로 CUDA를 씁니다.
- **미리보기 화질 개선** (`op-render/src/shaders/convert.wgsl`, `composite.wgsl`, `op-ui/src/monitor.rs`): 모니터가 줄여 보여 줄 때(4K를 작은 모니터에) 픽셀을 건너뛰며 샘플링해 계단·깜빡임이 생기던 것을 영역 평균(박스 필터)으로 바꿨습니다. 맞추기 보기에서 그려지는 프레임은 모니터보다 작아지지 않습니다. 시험: `a_reduced_picture_is_averaged_not_sampled`.
- **타임라인 높이 조절** (`op-ui/src/timeline.rs`, `session.rs`): ① 트랙 머리의 아래 경계를 아래로 끌면 위쪽 트랙이 커집니다(오디오 파형도 함께 커짐). ② 비디오 트랙과 오디오 트랙 사이의 막대를 위아래로 끌어 두 영역의 높이를 나눕니다. ③ 두 영역은 각자 세로 스크롤되고(마우스 휠은 포인터가 있는 영역), 트랙이 다 들어가지 않으면 오른쪽에 세로 스크롤 막대가 생깁니다. 높이는 프로젝트에 저장됩니다. 트랙 높이 키 명령(키울임·줄임·모두 확장)은 손으로 정한 높이를 지우고 공통 높이로 돌아갑니다.
- **컷 사이를 끌어 늘리기** (`timeline.rs`): 붙어 있는 두 클립의 컷(±4픽셀)을 누르고 끌면 롤링 편집이 됩니다(앞 클립이 컷 다음 프레임을 더 보여 주고 뒤 클립은 앞부분이 줄어듦). 가장자리 조절·이동이 누른 자리가 아니라 끌기가 시작된 자리로 잘못 판단되던 것도 고쳤습니다(egui는 몇 픽셀 움직인 뒤에야 끌기를 알려 줌: 이제 버튼을 누른 자리를 씁니다). 시험: `grabbing_the_cut_between_two_clips_is_a_rolling_edit_and_the_edges_still_trim`.
- **효과 컨트롤 오른쪽 클릭**: 효과 머리(모션 등)뿐 아니라 그 효과의 값 줄(위치 등)에서도 오른쪽 클릭하면 이 효과 복사 / 수정한 효과 모두 복사 / 복사한 효과 붙여넣기가 나옵니다. 타임라인 클립 오른쪽 클릭의 "복사한 효과 붙여넣기"는 고른 클립 개수를 보여 주며 항상 눌러지고, 복사한 것이 없으면 먼저 할 일을 알려 줍니다.
- **새 트랙에 끌어놓기** (`timeline.rs`, `commands.rs`, `app/mod.rs`): 프로젝트 패널이나 파일탐색기에서 파일을 끌어 타임라인으로 가져오면 맨 위(비디오)와 맨 아래(오디오)에 점선 "새 트랙" 칸이 나타나고, 거기에 놓으면 V4·A4 같은 새 트랙이 만들어져 그곳에 들어갑니다. 파일탐색기에서 끌어다 놓은 파일은 이제 프로젝트에만 들어가지 않고 놓은 자리(시간·트랙)에 타임라인에도 들어갑니다(Windows에서 마우스 위치를 시스템에 물어 판단). 시험: `media_dropped_on_the_empty_track_makes_a_new_track`, `an_empty_track_shows_while_media_is_dragged_over_the_timeline`.
- **타임라인 볼륨 선과 점** (`timeline.rs`): 오디오 클립(트랙 높이 40 이상, 보통 속도)에 흰 볼륨 선이 보이고, 선을 위아래로 끌면 클립 전체 볼륨, Ctrl+클릭(또는 점이 있는 클립에서 선을 끌기)으로 점이 추가되며, 점을 끌면 시간·크기가 바뀌고(0 dB 근처에서 붙음), Alt+클릭이나 오른쪽 클릭 > "볼륨 점 삭제"로 지웁니다. 점은 효과 컨트롤의 볼륨 키프레임과 같은 것입니다. 시험: `the_volume_line_takes_a_level_points_and_deletes_them`.
- **컷에 디졸브·크로스페이드 바로 넣기** (`transitions.rs`, `effects_clipboard.rs`, `timeline.rs`): 붙어 있는 두 클립의 컷을 더블클릭하면 영상 디졸브와 연결된 소리의 크로스페이드가 함께 들어가고, 컷을 오른쪽 클릭하면 "영상과 소리 모두 / 영상만 / 소리만"을 고를 수 있습니다. 시험: `a_dissolve_goes_on_the_cut_between_two_clips`.
- **내보내기 개선** (`export.rs`, `export_queue.rs`, `dialogs/export.rs`, `export_queue_panel.rs`): ① YouTube 사전 설정은 YouTube 권장값으로 자동 설정(4K 30fps 45 Mbps·60fps 68, 1440p 16·24, 1080p 8·12, 720p 5·8; 소리 AAC 384 kbps·48 kHz·스테레오; 키프레임 0.5초 간격·B프레임 2)되고 숫자를 바꾸면 직접 정한 값이 되며 "권장값" 버튼으로 돌아옵니다. ② "대기열에 추가"로 여러 내보내기를 쌓아 두고 창 > 내보내기 대기열에서 "대기열 시작"으로 한 개씩 차례로 내보냅니다(추가한 순간의 편집 상태로 저장). ③ "내보내는 동안 화면 미리보기" 체크박스(기본 끔): 미리보기를 만들 때마다 그래픽 카드에서 프레임을 읽어 오느라 렌더링이 멈추므로 끄면 더 빠릅니다. 시험: `the_youtube_presets_set_what_youtube_recommends`.
- **화면 색을 더 선명하게** (`theme.rs`, `timeline.rs`, `app/mod.rs`): 글자·강조색(파랑)을 밝고 진하게, 영상 클립을 선명한 청보라, 오디오 클립을 선명한 초록과 연두색 파형으로, 선택한 클립은 밝게 + 굵은 흰 테두리, 포커스가 있는 패널에 파란 테두리, 기본 트랙 높이를 키웠습니다.
- **비디오 트랙이 경계 막대를 따라 움직임** (`timeline.rs`): 비디오와 오디오 사이 막대를 내리면 비디오 트랙(V1이 막대에 붙은 채)도 같이 내려옵니다. 비디오 트랙 높이를 키우면 막대가 같이 내려가 아래 트랙이 밀려 내려갑니다. 비디오 영역의 세로 스크롤은 막대 쪽(V1)이 기준입니다.
- **효과 삭제·초기화·켜기/끄기를 선택한 클립 모두에** (`effects_clipboard.rs`, `effect_controls.rs`): 타임라인에서 클립 여러 개를 고르고 효과 컨트롤에서 효과 오른쪽 클릭 > 삭제(또는 처음 값으로), 효과 머리의 fx 버튼을 누르면 고른 클립 모두에서 같은 효과에 적용됩니다(되돌리기 한 번). 시험은 `changed_effects_are_copied_and_pasted_on_all_selected_clips_at_once`에 포함.
- **하드웨어 디코딩을 카드가 풀 수 있는지 확인해서 씀** (`op-media/src/video.rs`): 4:2:2·10비트처럼 카드마다 다른 영상은 첫 프레임을 카드로 풀어 보고(스트림 종류당 한 번) 풀리면 그 카드(NVIDIA RTX 50 시리즈는 4:2:2와 10비트 H.264를 하드웨어로 풀 수 있음)를 쓰고, 안 풀리면 프로세서가 모든 코어로 풉니다. RTX 3070에서는 후자임을 확인했고 RTX 50에서 풀리는지는 시험하지 못했습니다.
- **패널 탭 닫기**: 패널 탭을 오른쪽 클릭하면 "패널 닫기"가 나옵니다(창 메뉴에서 다시 열 수 있고, 마지막 패널은 닫히지 않습니다).
- **프로젝트 저장 확장자를 `.wedalur`로 변경** (`op-project/src/native.rs` 외): 새로 저장하는 프로젝트, 자동 저장, 복구 파일이 모두 `.wedalur`입니다. 예전 `.opproj` 파일은 계속 열립니다(열기 창·끌어다 놓기·AI 도구 `open_project`). `.opproj`로 열어 둔 프로젝트에서 저장하면 같은 이름의 `.wedalur`로 저장되고 예전 `.opproj` 파일은 그대로 남습니다(같은 이름의 `.wedalur`가 이미 있으면 덮어쓰지 않고 `.opproj`에 저장). 파일 형식(JSON)은 같습니다. 시험: `an_old_opproj_saves_as_wedalur_and_is_left_alone`, `both_extensions_name_a_project`.
- **타임라인 트랙 머리의 버튼이 눌리지 않던 오류 수정** (`op-ui/src/timeline.rs`): 트랙 머리 전체를 덮는 클릭 영역(빈 곳 메뉴)이 버튼들보다 나중에 만들어져, egui가 겹친 위젯 중 나중 것에 클릭을 주는 탓에 V2·V3·A2 등 트랙 대상(타깃) 버튼과 소스 패치, 동기 잠금, 트랙 잠금, 눈, 음소거(M), 솔로(S), 이름 더블클릭이 모두 반응하지 않았습니다. 빈 곳 영역을 먼저 만들도록 고쳤습니다(원본 OpenPremier의 오류). 시험: `crates/op-ui/tests/header_clicks.rs`(실제 egui로 클릭을 흘려 순서를 확인).
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
