"""OpenPremier 받아쓰기 도우미: Whisper large-v3 (faster-whisper, GPU) 로 말을 문장 단위로 받아 적는다.

사용: transcribe.py --pcm 소리.f32 [--language ko]
  --pcm      OpenPremier 가 만들어 준 16 kHz 모노 float32 소리 파일
결과: 표준출력에 JSON 한 줄  {"language", "duration", "sentences": [{"start", "end", "text"}]}  (시간은 초)
진행 상황은 표준에러에 한 줄씩 "progress 0.42" 로 적는다.
"""

import argparse
import json
import os
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
MODEL = Path(os.environ.get("WHISPER_DIR", HERE / "models" / "faster-whisper-large-v3"))
CUBLAS = HERE / "cuda" / "nvidia" / "cublas" / "bin"  # GPU 용 cuBLAS (pip install --target cuda nvidia-cublas-cu12)
RATE = 16000
SENTENCE_END = (".", "?", "!", "…", "。", "？", "！")

if CUBLAS.is_dir():
    os.add_dll_directory(str(CUBLAS))
    os.environ["PATH"] = str(CUBLAS) + os.pathsep + os.environ["PATH"]


def load_model(folder):
    from faster_whisper import WhisperModel

    folder = Path(folder or MODEL)
    if not (folder / "model.bin").exists():
        sys.exit(f"Whisper 모델이 없습니다: {folder}")
    try:
        return WhisperModel(str(folder), device="cuda", compute_type="float16")
    except Exception as e:  # GPU/CUDA 라이브러리 문제면 느려도 CPU 로
        print(f"CUDA 실패, CPU 로 전환: {e}", file=sys.stderr)
        return WhisperModel(str(folder), device="cpu", compute_type="int8")


def sentences(segments, duration):
    """단어 시간으로 문장 끝(마침표 등)에서 끊고, 구간 끝도 문장 끝으로 본다."""
    out, cur = [], []

    def words_of(tokens):
        """Whisper 조각을 띄어쓰기(앞 공백) 기준 어절로 묶는다."""
        words = []
        for s, e, t in tokens:
            if t.startswith(" ") or not words:
                words.append({"start": round(s, 3), "end": round(e, 3), "text": t.strip()})
            else:
                words[-1]["end"] = round(e, 3)
                words[-1]["text"] += t
        words = [w for w in words if w["text"]]
        # Whisper 는 말 사이 침묵을 앞 단어 길이에 넣곤 한다(한 단어가 20초로 잡히기도 함).
        # 글자 수에 맞는 길이로 줄여, 자막이 침묵 내내 남거나 삭제 구간이 부풀지 않게 한다.
        for w in words:
            w["end"] = round(min(w["end"], w["start"] + 0.4 + 0.15 * len(w["text"])), 3)
        return words

    def flush():
        text = "".join(w for _, _, w in cur).strip()
        words = words_of(cur)
        if text and words:
            out.append({"start": words[0]["start"], "end": words[-1]["end"], "text": text, "words": words})
        cur.clear()

    for seg in segments:
        for w in seg.words or []:
            cur.append((w.start, w.end, w.word))
            if w.word.rstrip().endswith(SENTENCE_END):
                flush()
        if not seg.words:
            cur.append((seg.start, seg.end, seg.text))
        flush()
        print(f"progress {min(seg.end / max(duration, 1e-9), 1.0):.3f}", file=sys.stderr, flush=True)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pcm", required=True)
    ap.add_argument("--language", default=None)
    ap.add_argument("--model", default=None, help="folder of a faster-whisper model (default: large-v3 in models/)")
    args = ap.parse_args()
    audio = np.fromfile(args.pcm, dtype="<f4")
    duration = len(audio) / RATE
    model = load_model(args.model)
    segments, info = model.transcribe(audio, language=args.language or None, word_timestamps=True, vad_filter=True,
                                      condition_on_previous_text=False, beam_size=5)
    result = {"language": info.language, "duration": round(duration, 3), "sentences": sentences(segments, duration)}
    sys.stdout.buffer.write(json.dumps(result, ensure_ascii=False).encode("utf-8") + b"\n")


if __name__ == "__main__":
    main()
