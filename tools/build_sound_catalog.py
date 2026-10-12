"""Builds crates/op-application/data/sounds.json, the catalog of the sound library.

Only public-domain (CC0) sources whose license text was read are listed. Every file is
addressed by a URL pinned to a commit of the source repository, so a listed sound never changes.
Run:  python tools/build_sound_catalog.py   (needs the `gh` command, logged in, for the GitHub API)
"""
import csv
import io
import json
import subprocess
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "crates" / "op-application" / "data" / "sounds.json"

# source id -> (repository, pinned commit, display name, license, where the license was read)
SOURCES = {
    "kenney": ("Mcamento8/open-game-sfx-index", "34bbe8b525b5ceff3804915ea745532af11f6055",
               "Kenney (kenney.nl), index by Mcamento8", "CC0-1.0",
               "https://kenney.nl/assets/category:Audio"),
    "reza": ("RezaParsian/free-sfx-bgm-pack", "f8d463c05c814804b786f9c8a5683d1abcd954da",
             "RezaParsian/free-sfx-bgm-pack (synthesized from code)", "CC0-1.0",
             "https://github.com/RezaParsian/free-sfx-bgm-pack/blob/f8d463c05c814804b786f9c8a5683d1abcd954da/LICENSE"),
    "aeris": ("farmmer938-lgtm/aeris-cc0-music", "51ab767ca32558ae5686344fc3b29c7a4498b145",
              "AERIS MUSIC (farmmer938-lgtm/aeris-cc0-music)", "CC0-1.0",
              "https://github.com/farmmer938-lgtm/aeris-cc0-music/blob/51ab767ca32558ae5686344fc3b29c7a4498b145/LICENSE"),
}


def raw(repo, sha, path):
    return subprocess.run(
        ["gh", "api", "-H", "Accept: application/vnd.github.raw", f"repos/{repo}/contents/{path}?ref={sha}"],
        check=True, capture_output=True).stdout


sounds = []


def add(src, sid, name, ko, kind, cat, tags, sec, path):
    sounds.append({"id": f"{src}/{sid}", "name": name, "ko": ko, "kind": kind, "cat": cat,
                   "tags": tags, "sec": round(sec, 2), "src": src, "path": path})


# ------------------------------------------------------------------ Kenney (CC0 packs)
KENNEY = {  # pack -> (kind, Korean category)
    "impact-sounds": ("sfx", "충격·타격"),
    "interface-sounds": ("sfx", "인터페이스"),
    "ui-audio": ("sfx", "클릭·UI"),
    "digital-audio": ("sfx", "디지털 비프"),
    "sci-fi-sounds": ("sfx", "SF·레이저"),
    "rpg-audio": ("sfx", "RPG·장비"),
    "casino-audio": ("sfx", "카드·카지노"),
    "voiceover-pack": ("sfx", "영어 보이스"),
    "voiceover-pack-fighter": ("sfx", "영어 보이스"),
    "music-jingles": ("bgm", "징글(짧은 음악)"),
}
index = json.loads(raw(*SOURCES["kenney"][:2], "index.json"))
for s in index["sounds"]:
    if s["pack"] not in KENNEY or not s["source"].startswith("https://kenney.nl/"):
        continue
    kind, cat = KENNEY[s["pack"]]
    path = s.get("file_ogg") or s.get("file_wav") or s.get("file_mp3") or s.get("file_flac")
    if not path:
        continue
    add("kenney", s["id"], s["title"], "", kind, cat, " ".join(s.get("tags", [])), s["duration_sec"], path)

# ------------------------------------------------------------------ Reza (synthesized, CC0)
REZA_KO = {
    "air_horn_meme": ("에어혼 (밈)", "임팩트·강조"), "boom_cinematic": ("시네마틱 붐", "임팩트·강조"),
    "impact_bright": ("밝은 임팩트", "임팩트·강조"), "impact_deep": ("묵직한 임팩트", "임팩트·강조"),
    "gong_section": ("징 (구간 전환)", "임팩트·강조"), "logo_sting": ("로고 스팅", "임팩트·강조"),
    "whoosh_short": ("짧은 휘익", "전환·휘익"), "whoosh_transition": ("전환 휘익", "전환·휘익"),
    "rewind_sweep": ("되감기 스윕", "전환·휘익"), "swipe_paper": ("종이 넘김", "전환·휘익"),
    "riser_tension": ("긴장 상승음", "전환·휘익"), "scroll_ratchet": ("스크롤 틱틱", "전환·휘익"),
    "notification_ding": ("알림 딩동", "알림·팝"), "success_chime": ("성공 차임", "알림·팝"),
    "coin_ping": ("코인 핑", "알림·팝"), "error_buzz": ("오류 부저", "알림·팝"),
    "pop_bubble": ("방울 톡", "알림·팝"), "sparkle_magic": ("반짝 마법", "알림·팝"),
    "camera_shutter": ("카메라 셔터", "알림·팝"), "text_reveal_soft": ("글자 나타남", "알림·팝"),
    "terminal_bell": ("벨 소리", "알림·팝"), "laser_pew": ("레이저 슝", "알림·팝"),
    "ui_click": ("클릭", "클릭·카운트"), "ui_double_click": ("더블 클릭", "클릭·카운트"),
    "ui_hover_soft": ("부드러운 호버", "클릭·카운트"), "cut_tick": ("컷 틱", "클릭·카운트"),
    "prompt_tick": ("프롬프트 틱", "클릭·카운트"), "countdown_beeps": ("카운트다운 삑삑", "클릭·카운트"),
    "countdown_tick_single": ("카운트다운 틱", "클릭·카운트"),
    "type_backspace": ("백스페이스", "타자·키보드"), "type_burst_mech": ("기계식 타자 연타", "타자·키보드"),
    "type_burst_soft": ("부드러운 타자 연타", "타자·키보드"), "type_enter": ("엔터 키", "타자·키보드"),
    "type_key_roll": ("키 연타", "타자·키보드"), "type_key_single": ("키 한 번", "타자·키보드"),
    "typing_keyboard": ("키보드 타자", "타자·키보드"),
    "heartbeat_tension": ("심장 박동", "분위기·루프"), "room_tone_loop": ("방 소음 (루프)", "분위기·루프"),
    "vinyl_crackle_loop": ("LP 지직 (루프)", "분위기·루프"), "vinyl_stop": ("LP 멈춤", "분위기·루프"),
}
REZA_BGM_KO = {
    "bgm_ambient_underscore": ("앰비언트 배경", "차분·앰비언트"), "bgm_cinematic_tension": ("시네마틱 긴장", "시네마틱·어두움"),
    "bgm_hook_loop_8s": ("훅 루프 8초", "밝음·브이로그"), "bgm_lofi_focus": ("로파이 집중", "차분·앰비언트"),
    "bgm_playful_marimba": ("장난스러운 마림바", "밝음·브이로그"), "bgm_screen_bed": ("화면 설명 배경", "테크·설명"),
    "bgm_tech_explainer": ("테크 설명", "테크·설명"), "bgm_terminal_flow": ("터미널 플로우", "테크·설명"),
    "bgm_upbeat_vlog": ("경쾌한 브이로그", "밝음·브이로그"),
}
rows = list(csv.DictReader(io.StringIO(raw(*SOURCES["reza"][:2], "audio-pack/measured.csv").decode("utf-8"))))
for r in rows:
    name, kind = r["name"], r["type"]
    ko, cat = (REZA_KO if kind == "sfx" else REZA_BGM_KO)[name]
    add("reza", name, name.replace("_", " "), ko, kind, cat, name.replace("_", " "), float(r["duration_sec"]),
        f"audio-pack/mp3/{kind}/{name}.mp3")

# ------------------------------------------------------------------ AERIS MUSIC (CC0 tracks)
AERIS = [  # file, title, Korean category, seconds (from the README table)
    ("signal-lost", "Signal Lost", "시네마틱·어두움", 80), ("beneath", "Beneath", "시네마틱·어두움", 80),
    ("the-long-walk-signal-lost", "The Long Walk (Signal Lost)", "시네마틱·어두움", 50),
    ("horizon-monolith", "Horizon Monolith", "밝음·브이로그", 69), ("evergreen-open-road", "Evergreen (Open Road)", "밝음·브이로그", 39),
    ("overdrive-signal-lost", "Overdrive Signal Lost", "경쾌·에너지", 60), ("ash", "Ash", "시네마틱·어두움", 87),
    ("colder-air", "Colder Air", "시네마틱·어두움", 55), ("afterglow-vast", "Afterglow Vast", "차분·앰비언트", 91),
    ("quiet-room", "Quiet Room", "차분·앰비언트", 57), ("bedroom-window-outlands", "Bedroom Window (Outlands)", "차분·앰비언트", 58),
    ("bedroom-window-vast", "Bedroom Window Vast", "차분·앰비언트", 104), ("old-photos-outlands", "Old Photos (Outlands)", "차분·앰비언트", 104),
]
for f, title, cat, sec in AERIS:
    add("aeris", f, title, "", "bgm", cat, title.lower(), sec, f"tracks/{f}.mp3")

# editing sounds with Korean names first, then music, then the large game packs (sort is stable)
sounds.sort(key=lambda x: {"reza": 0, "aeris": 1, "kenney": 2}[x["src"]])

catalog = {
    "sources": {k: {"repo": v[0], "commit": v[1], "name": v[2], "license": v[3], "proof": v[4]} for k, v in SOURCES.items()},
    "sounds": sounds,
}
OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text(json.dumps(catalog, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
by = {}
for s in sounds:
    by.setdefault((s["kind"], s["cat"]), 0)
    by[(s["kind"], s["cat"])] += 1
print(len(sounds), "sounds,", OUT.stat().st_size // 1024, "KB", file=sys.stderr)
for k, v in sorted(by.items()):
    print(k, v, file=sys.stderr)
