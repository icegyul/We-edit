# SPEC-10 SRT 자막 내보내기 (유튜브에 자막 올리기)

> 사용자 요청(2026-10-14): "내보내기 → SRT 자막 내보내기 만들어줘", 그리고 "유튜브에 자막 올라갈 수 있게".

## 0. 현재 상태 (코드로 확인)
- 자막 내보내기는 **이미 있다**: `op-application/src/captions.rs`의 `export_captions`(자막 클립 → SRT 또는 `.vtt`), `op-core/src/captions.rs`의 `to_srt`.
- 그러나 **위치가 숨어 있다**: 메뉴 `그래픽 > 자막 > Export Captions File...`(`op.captions.export`, `op-ui/src/app/menus.rs` 203행 부근). 내보내기 메뉴(`menus.rs` 77행 `Export`)에는 없다.
- **자막 클립이 없으면 실패한다**: "the sequence has no captions". 받아쓰기 문장(`ed.transcript`)은 쓰지 않는다.
- **유튜브 규격 검사가 없다**: 겹치는 시간, 빈 자막, HTML 태그, 한 줄 길이, 세 줄 이상 자막을 걸러내지 않는다. `to_srt`는 형식만 맞춘다.

## 1. 메뉴 위치
- **파일(내보내기) > SRT 자막 내보내기…** 를 `Export` 메뉴 아래쪽에 추가한다.
- 기존 `그래픽 > 자막 > 자막 파일 내보내기`는 **같은 명령(`op.captions.export`)** 을 그대로 쓴다. 두 메뉴가 같은 함수를 호출하게 해서 로직을 하나로 둔다.
- 번역: 영어 "Export Subtitles (SRT)...", 한국어 "SRT 자막 내보내기…", 스페인어는 `es.rs`에 직접.

## 2. 자막 원본 선택
- 대화창 "출처" 항목:
  - **자막 클립**(기본, 자막 클립이 있을 때)
  - **받아쓰기 문장**(받아쓰기 결과 `ed.transcript`를 자막으로 쓴다)
- 자막 클립도 받아쓰기도 없으면 "먼저 자막을 만들거나 받아쓰기를 하세요" 안내를 띄운다(눌러도 반응 없는 버튼 금지, `00` 규칙 5).
- 받아쓰기 문장 → 자막 변환: 문장의 시작·끝 시간을 그대로 쓴다. 긴 문장은 §3의 줄 나눔을 거친다.

## 3. 유튜브 규격 검사·정리 (`op-core/src/captions.rs`)
근거: YouTube 자막 업로드 안내(SRT는 기본 형식, UTF-8, 태그 금지, 시간 겹침 금지), 자막 가이드(한 줄 약 40~42자, 최대 2줄). 줄 길이의 **유튜브 공식 한도는 찾지 못했다**. 아래 42자는 가이드 값이며 옵션으로 바꿀 수 있게 한다.

| 항목 | 규칙 | 처리 |
|---|---|---|
| 인코딩 | UTF-8 | 기본은 BOM 없음. 옵션 "BOM 넣기"(일부 메모장 호환) |
| 형식 | 번호 1부터 연속, `hh:mm:ss,mmm --> hh:mm:ss,mmm`, 텍스트, 빈 줄 | 현재 구현과 같음 |
| 태그 | HTML·스타일 태그 금지 | `<…>` 제거 |
| 겹침 | 자막은 다음 자막이 시작하기 전에 끝난다 | 앞 자막 끝을 다음 시작으로 자른다 |
| 길이 | 끝 > 시작, 0.3초 미만 금지 | 0.3초까지 늘리되 다음 자막과 겹치지 않게. 불가능하면 경고 |
| 빈 자막 | 텍스트 없는 자막 금지 | 제거하고 보고 |
| 줄 길이 | 한 줄 최대 42자(옵션) | 공백에서 줄 나눔 |
| 줄 수 | 한 자막 최대 2줄(옵션) | 3줄 이상이면 시간을 나눠 다음 자막으로 |
| 시간 | 음수 금지 | 0으로 |
| 줄바꿈 | LF 또는 CRLF | CRLF(윈도우 호환) |

- 검사 결과를 상태줄과 대화창에 보고한다: `자막 128개 저장 · 겹침 6곳 조정 · 줄 나눔 14곳 · 빈 자막 2개 제거 · 0.3초 미만 3곳 조정`.
- 함수: `to_srt_checked(cues, &SrtOptions) -> (String, Report)`. `to_srt`는 기존 시험을 위해 남긴다.

## 4. 대화창 (내보내기 전 옵션)
- 출처: 자막 클립 / 받아쓰기 문장
- 한 줄 길이: 42자(기본), 최대 줄 수: 2(기본)
- 시간 기준: "시퀀스 시작을 0초로"(기본)
- BOM 넣기: 끔(기본)
- 파일 이름: `<시퀀스 이름>.srt` 제안(언어 코드 붙이기 옵션: `.ko.srt`)
- 저장 버튼 → 저장 후 결과 보고 → **"유튜브에 올리는 법" 보기** 버튼(§5 문구를 띄운다)

## 5. 유튜브 업로드 안내 (앱 안 문구)
1. YouTube 스튜디오에서 영상 선택 → **자막** → **언어 추가** → 자막 언어 선택
2. **파일 업로드** → 저장한 `.srt` 선택
3. **"시간 정보 포함"** 을 고른다(SRT는 시간이 들어 있음)
4. 미리보기에서 자막 싱크를 확인한다. 어긋나면 해당 자막을 직접 고친다

## 6. MCP 도구 (`op-mcp/src/tools.rs`)
- `export_captions {path, source: "captions"|"transcript", max_chars?, max_lines?, bom?}` → `{count, changed: {overlap, split, empty_removed, lengthened}, path}`.
- 도구 설명과 AI 패널 안내(`ai_panel.rs` PROMPT)에 한 줄: "유튜브용 자막은 export_captions로 SRT를 만든다. 겹침·줄 길이는 자동으로 맞춘다."

## 7. 자동 시험
- `op-core` 단위 시험:
  - `overlaps_are_trimmed_to_the_next_start`
  - `html_tags_are_removed`
  - `long_lines_wrap_at_spaces_to_42_chars`
  - `more_than_two_lines_split_into_the_next_caption`
  - `empty_captions_are_dropped`
  - `short_captions_are_lengthened_without_overlap`
  - `utf8_round_trip_keeps_korean_text`
  - `bom_option_writes_the_mark`
- `srt_validator(text)`(시험용 검사기): 번호 연속, 시간 형식(정규식), 끝 > 시작, 겹침 없음, 줄 ≤42자, ≤2줄, 태그 없음, UTF-8. **모든 출력 시험에 이 검사기를 적용한다.**
- `op-application` 시험: 받아쓰기 문장 → SRT, 자막 클립 → SRT, 둘 다 없으면 안내 오류.
- `op-mcp` 시험: `export_captions` 결과 JSON과 파일이 검사기를 통과하는지.

## 8. 수동 확인 (사람이 해야 함, 자동으로 대신할 수 없음)
- 사용자 영상으로 SRT를 만든다 → 유튜브에 **일부 공개**로 올린다 → 자막 미리보기에서 싱크와 줄 나눔을 확인한다.
- 확인 결과(OK/어긋남 시각)를 `docs/dev/09-접수된-문제와-수정지시.md`에 기록한다.

## 9. 순서와 규모 (M)
1. `op-core`: 검사·정리 함수와 시험 (핵심, 유튜브 규격)
2. `op-application`: 받아쓰기 문장 출처, 안내 오류
3. UI: 메뉴 위치, 옵션 대화상자, 업로드 안내
4. MCP 도구와 AI 안내
5. 자동 시험 전체 통과 → 릴리스 빌드 → 실제 창에서 내보내기 → 사용자의 유튜브 일부 공개 확인

## 10. 근거 자료
- 자막 업로드 방법과 형식: https://support.trueanthem.com/hc/en-us/articles/47493388484372-Upload-and-Format-Captions-SRT-for-YouTube-Facebook-Videos
- 자막 업로드 절차: https://openlab.sps.cuny.edu/sps-faculty-community-hub/how-to-upload-a-caption-file-srt-to-youtube
- 자막 파일 형식 안내: https://www.jmu.edu/accessibility/digital-accessibility/guides/captioning/youtube/upload-a-captioning-file.shtml
- YouTube 지원 자막 형식 안내(공식 도움말, 검색 결과에서 확인): https://support.google.com/youtube/answer/2734698?hl=en
- 겹치는 시간과 SBV→SRT 변환: https://trac.ffmpeg.org/wiki/HowToConvertYouTubeSBVtoSRT
